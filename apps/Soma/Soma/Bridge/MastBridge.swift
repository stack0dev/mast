import Foundation
import CMast

// MARK: - Error

enum MastBridgeError: Error, LocalizedError {
    case ffiError(code: Int32, message: String)
    case notOpen

    var errorDescription: String? {
        switch self {
        case .ffiError(_, let message): message
        case .notOpen: "Mast database is not open"
        }
    }
}

// MARK: - Embedder Context

/// Class-based context so we can pass a stable pointer through C void*.
/// Prevent premature deallocation by retaining via Unmanaged.
private final class EmbedderContext {
    let embedFn: (String) async -> [Float]
    let dims: Int

    init(embed: @escaping (String) async -> [Float], dimensions: Int) {
        self.embedFn = embed
        self.dims = dimensions
    }
}

// MARK: - Compactor Context

private final class CompactorContext {
    let synthesizeFn: ([String]) async -> String

    init(synthesize: @escaping ([String]) async -> String) {
        self.synthesizeFn = synthesize
    }
}

// MARK: - Bridge Actor

actor MastBridge {
    private var handle: OpaquePointer?

    // MARK: - Lifecycle

    func open(path: String) throws {
        if handle != nil {
            close()
        }
        var errPtr: UnsafeMutablePointer<MastMastFfiError>?
        let h = path.withCString { cPath in
            mast_open(cPath, &errPtr)
        }
        try checkError(errPtr)
        handle = h
    }

    func close() {
        guard let h = handle else { return }
        mast_close(h)
        handle = nil
    }

    deinit {
        if let h = handle {
            mast_close(h)
        }
    }

    // MARK: - Store

    func store(
        collection: String,
        content: String,
        metadata: [String: String] = [:],
        tier: MastTier = .active,
        ttl: Int64? = nil,
        embed: @escaping (String) async -> [Float],
        dimensions: Int
    ) throws -> String {
        let h = try requireHandle()

        let vtable = buildEmbedderVtable(embed: embed, dimensions: dimensions)

        let metaKeys = Array(metadata.keys)
        let metaValues = Array(metadata.values)

        return try withCStringsArray(metaKeys) { cKeys in
            try withCStringsArray(metaValues) { cValues in
                try collection.withCString { cCollection in
                    try content.withCString { cContent in
                        try tier.rawValue.withCString { cTier in
                            var meta = MastMastMetadata(
                                keys: cKeys,
                                values: cValues,
                                count: metadata.count
                            )
                            var localVtable = vtable.vtable
                            var errPtr: UnsafeMutablePointer<MastMastFfiError>?
                            let result = mast_store(
                                h,
                                cCollection,
                                cContent,
                                &localVtable,
                                &meta,
                                cTier,
                                ttl ?? 0,
                                ttl != nil,
                                &errPtr
                            )
                            defer { vtable.release() }
                            try checkError(errPtr)
                            guard let result else {
                                throw MastBridgeError.ffiError(code: -1, message: "store returned null")
                            }
                            defer { mast_free_store_result(result) }
                            return String(cString: result.pointee.id)
                        }
                    }
                }
            }
        }
    }

    // MARK: - List

    func list(collection: String) throws -> [MastMemory] {
        let h = try requireHandle()
        var errPtr: UnsafeMutablePointer<MastMastFfiError>?
        let result = collection.withCString { cCol in
            mast_list(h, cCol, &errPtr)
        }
        try checkError(errPtr)
        guard let result else { return [] }
        defer { mast_free_list_result(result) }

        let count = result.pointee.count
        guard count > 0, let entries = result.pointee.entries else { return [] }

        var memories: [MastMemory] = []
        memories.reserveCapacity(count)
        for i in 0..<count {
            let entry = entries[i]
            memories.append(convertMemoryEntry(entry))
        }
        return memories
    }

    // MARK: - Recall

    func recall(
        collection: String,
        query: String? = nil,
        textQuery: String? = nil,
        mode: MastSearchMode = .vector,
        limit: Int = 10,
        filterJSON: String? = nil,
        embed: @escaping (String) async -> [Float],
        dimensions: Int
    ) throws -> [MastRecallResult] {
        let h = try requireHandle()

        let vtable = buildEmbedderVtable(embed: embed, dimensions: dimensions)
        defer { vtable.release() }

        var localVtable = vtable.vtable
        var errPtr: UnsafeMutablePointer<MastMastFfiError>?

        let result = withOptionalCString(query) { cQuery in
            withOptionalCString(textQuery) { cTextQuery in
                withOptionalCString(filterJSON) { cFilter in
                    collection.withCString { cCol in
                        mode.cString.withCString { cMode in
                            mast_recall_full(
                                h,
                                cCol,
                                cQuery,
                                cTextQuery,
                                &localVtable,
                                cMode,
                                mode.vectorWeight,
                                mode.textWeight,
                                limit,
                                cFilter,
                                &errPtr
                            )
                        }
                    }
                }
            }
        }

        try checkError(errPtr)
        guard let result else { return [] }
        defer { mast_free_recall_full_result(result) }

        let count = result.pointee.count
        guard count > 0, let entries = result.pointee.entries else { return [] }

        var results: [MastRecallResult] = []
        results.reserveCapacity(count)
        for i in 0..<count {
            let entry = entries[i]
            results.append(convertRecallFullEntry(entry))
        }
        return results
    }

    // MARK: - Get

    func get(collection: String, id: String) throws -> String? {
        let h = try requireHandle()
        var outContent: UnsafeMutablePointer<CChar>?
        var errPtr: UnsafeMutablePointer<MastMastFfiError>?

        let found = collection.withCString { cCol in
            id.withCString { cId in
                mast_get(h, cCol, cId, &outContent, &errPtr)
            }
        }

        try checkError(errPtr)
        guard found, let outContent else { return nil }
        defer { mast_free_string(outContent) }
        return String(cString: outContent)
    }

    // MARK: - Delete

    func delete(collection: String, id: String) throws {
        let h = try requireHandle()
        var errPtr: UnsafeMutablePointer<MastMastFfiError>?

        collection.withCString { cCol in
            id.withCString { cId in
                _ = mast_delete(h, cCol, cId, &errPtr)
            }
        }

        try checkError(errPtr)
    }

    // MARK: - Relate

    func relate(
        collection: String,
        source: String,
        target: String,
        relation: String,
        weight: Float
    ) throws -> String {
        let h = try requireHandle()
        var outId: UnsafeMutablePointer<CChar>?
        var errPtr: UnsafeMutablePointer<MastMastFfiError>?

        collection.withCString { cCol in
            source.withCString { cSrc in
                target.withCString { cTgt in
                    relation.withCString { cRel in
                        _ = mast_relate(h, cCol, cSrc, cTgt, cRel, weight, &outId, &errPtr)
                    }
                }
            }
        }

        try checkError(errPtr)
        guard let outId else {
            throw MastBridgeError.ffiError(code: -1, message: "relate returned null id")
        }
        defer { mast_free_string(outId) }
        return String(cString: outId)
    }

    // MARK: - Unrelate

    func unrelate(
        collection: String,
        source: String,
        target: String,
        relation: String
    ) throws {
        let h = try requireHandle()
        var errPtr: UnsafeMutablePointer<MastMastFfiError>?

        collection.withCString { cCol in
            source.withCString { cSrc in
                target.withCString { cTgt in
                    relation.withCString { cRel in
                        _ = mast_unrelate(h, cCol, cSrc, cTgt, cRel, &errPtr)
                    }
                }
            }
        }

        try checkError(errPtr)
    }

    // MARK: - Compact

    func compact(
        collection: String,
        sourceTier: MastTier,
        targetTier: MastTier,
        strategy: MastCompactionStrategy,
        maxBatchSize: Int,
        dryRun: Bool,
        synthesize: @escaping ([String]) async -> String,
        embed: @escaping (String) async -> [Float],
        dimensions: Int
    ) throws -> MastCompactResult {
        let h = try requireHandle()

        let embedVtable = buildEmbedderVtable(embed: embed, dimensions: dimensions)
        defer { embedVtable.release() }

        let compactVtable = buildCompactorVtable(synthesize: synthesize)
        defer { compactVtable.release() }

        var localEmbedVtable = embedVtable.vtable
        var localCompactVtable = compactVtable.vtable
        var errPtr: UnsafeMutablePointer<MastMastFfiError>?

        let result = collection.withCString { cCol in
            sourceTier.rawValue.withCString { cSrc in
                targetTier.rawValue.withCString { cTgt in
                    strategy.cString.withCString { cStrategy in
                        mast_compact(
                            h,
                            cCol,
                            cSrc,
                            cTgt,
                            cStrategy,
                            maxBatchSize,
                            dryRun,
                            &localCompactVtable,
                            &localEmbedVtable,
                            &errPtr
                        )
                    }
                }
            }
        }

        try checkError(errPtr)
        guard let result else {
            throw MastBridgeError.ffiError(code: -1, message: "compact returned null")
        }
        defer { mast_free_compact_result(result) }

        let r = result.pointee
        let newIds = extractStringArray(ptr: r.new_ids, count: r.new_ids_count)
        let sourceIds = extractStringArray(ptr: r.source_ids, count: r.source_ids_count)

        return MastCompactResult(
            sourceCount: r.source_count,
            compactedCount: r.compacted_count,
            newIds: newIds,
            sourceIds: sourceIds
        )
    }

    // MARK: - Traverse

    func traverse(
        collection: String,
        start: String,
        maxDepth: Int,
        relationFilter: String? = nil,
        minWeight: Float? = nil
    ) throws -> [MastEdge] {
        let h = try requireHandle()
        var errPtr: UnsafeMutablePointer<MastMastFfiError>?

        let result = withOptionalCString(relationFilter) { cRelFilter in
            collection.withCString { cCol in
                start.withCString { cStart in
                    mast_traverse(
                        h,
                        cCol,
                        cStart,
                        maxDepth,
                        cRelFilter,
                        minWeight ?? 0.0,
                        minWeight != nil,
                        &errPtr
                    )
                }
            }
        }

        try checkError(errPtr)
        return convertGraphResult(result)
    }

    // MARK: - Subgraph

    func subgraph(collection: String) throws -> [MastEdge] {
        let h = try requireHandle()
        var errPtr: UnsafeMutablePointer<MastMastFfiError>?

        let result = collection.withCString { cCol in
            mast_subgraph(h, cCol, &errPtr)
        }

        try checkError(errPtr)
        return convertGraphResult(result)
    }

    // MARK: - Snapshot

    func snapshot(
        collection: String,
        outputPath: String
    ) throws -> (memoryCount: UInt64, edgeCount: UInt64) {
        let h = try requireHandle()
        var errPtr: UnsafeMutablePointer<MastMastFfiError>?

        let result = collection.withCString { cCol in
            outputPath.withCString { cPath in
                mast_snapshot(h, cCol, cPath, &errPtr)
            }
        }

        try checkError(errPtr)
        guard let result else {
            throw MastBridgeError.ffiError(code: -1, message: "snapshot returned null")
        }
        defer { mast_free_snapshot_result(result) }
        return (memoryCount: result.pointee.memory_count, edgeCount: result.pointee.edge_count)
    }

    // MARK: - Restore

    func restore(
        inputPath: String,
        merge: Bool
    ) throws -> (memoryCount: UInt64, edgeCount: UInt64) {
        let h = try requireHandle()
        var errPtr: UnsafeMutablePointer<MastMastFfiError>?

        let result = inputPath.withCString { cPath in
            mast_restore(h, cPath, merge, &errPtr)
        }

        try checkError(errPtr)
        guard let result else {
            throw MastBridgeError.ffiError(code: -1, message: "restore returned null")
        }
        defer { mast_free_restore_result(result) }
        return (memoryCount: result.pointee.memory_count, edgeCount: result.pointee.edge_count)
    }
}

// MARK: - Private Helpers

extension MastBridge {

    private func requireHandle() throws -> OpaquePointer {
        guard let h = handle else { throw MastBridgeError.notOpen }
        return h
    }

    private func checkError(_ errPtr: UnsafeMutablePointer<MastMastFfiError>?) throws {
        guard let errPtr else { return }
        defer { mast_free_error(errPtr) }
        let code = errPtr.pointee.code
        let message: String
        if let msgPtr = errPtr.pointee.message {
            message = String(cString: msgPtr)
        } else {
            message = "unknown FFI error (code \(code))"
        }
        throw MastBridgeError.ffiError(code: code, message: message)
    }

    /// Convert an optional String to a C string pointer, passing nil as NULL.
    private func withOptionalCString<R>(
        _ string: String?,
        body: (UnsafePointer<CChar>?) -> R
    ) -> R {
        if let string {
            return string.withCString { body($0) }
        } else {
            return body(nil)
        }
    }

    /// Temporarily create a C array of C strings from a Swift string array.
    private func withCStringsArray<R>(
        _ strings: [String],
        body: (UnsafePointer<UnsafePointer<CChar>?>?) throws -> R
    ) rethrows -> R {
        guard !strings.isEmpty else {
            return try body(nil)
        }
        // Allocate CString copies that live for the duration of the closure.
        var cStrings = strings.map { strdup($0) }
        defer {
            for ptr in cStrings { free(ptr) }
        }
        return try cStrings.withUnsafeMutableBufferPointer { buffer in
            // Reinterpret UnsafeMutablePointer<CChar>? as UnsafePointer<CChar>?
            try buffer.baseAddress!.withMemoryRebound(
                to: UnsafePointer<CChar>?.self,
                capacity: strings.count
            ) { rebound in
                try body(rebound)
            }
        }
    }

    private func extractStringArray(
        ptr: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?,
        count: Int
    ) -> [String] {
        guard let ptr, count > 0 else { return [] }
        var result: [String] = []
        result.reserveCapacity(count)
        for i in 0..<count {
            if let cStr = ptr[i] {
                result.append(String(cString: cStr))
            }
        }
        return result
    }

    private func parseMetadataJSON(_ jsonPtr: UnsafeMutablePointer<CChar>?) -> [String: String] {
        guard let jsonPtr else { return [:] }
        let json = String(cString: jsonPtr)
        guard !json.isEmpty,
              let data = json.data(using: .utf8),
              let dict = try? JSONSerialization.jsonObject(with: data) as? [String: String]
        else { return [:] }
        return dict
    }

    private func convertMemoryEntry(_ entry: MastMastMemoryEntry) -> MastMemory {
        MastMemory(
            id: String(cString: entry.id),
            content: String(cString: entry.content),
            tier: MastTier(rawValue: String(cString: entry.tier)) ?? .active,
            createdAt: Date(timeIntervalSince1970: TimeInterval(entry.created_at)),
            accessCount: entry.access_count,
            metadata: parseMetadataJSON(entry.metadata_json)
        )
    }

    private func convertRecallFullEntry(_ entry: MastMastRecallFullEntry) -> MastRecallResult {
        MastRecallResult(
            id: String(cString: entry.id),
            content: String(cString: entry.content),
            score: entry.score,
            tier: MastTier(rawValue: String(cString: entry.tier)) ?? .active,
            createdAt: Date(timeIntervalSince1970: TimeInterval(entry.created_at)),
            accessCount: entry.access_count,
            metadata: parseMetadataJSON(entry.metadata_json)
        )
    }

    private func convertEdgeEntry(_ entry: MastMastEdgeEntry) -> MastEdge {
        MastEdge(
            id: String(cString: entry.id),
            source: String(cString: entry.source),
            target: String(cString: entry.target),
            relation: String(cString: entry.relation),
            weight: entry.weight,
            createdAt: Date(timeIntervalSince1970: TimeInterval(entry.created_at)),
            updatedAt: Date(timeIntervalSince1970: TimeInterval(entry.updated_at))
        )
    }

    private func convertGraphResult(
        _ result: UnsafeMutablePointer<MastMastGraphResult>?
    ) -> [MastEdge] {
        guard let result else { return [] }
        defer { mast_free_graph_result(result) }

        let count = result.pointee.count
        guard count > 0, let entries = result.pointee.edges else { return [] }

        var edges: [MastEdge] = []
        edges.reserveCapacity(count)
        for i in 0..<count {
            edges.append(convertEdgeEntry(entries[i]))
        }
        return edges
    }
}

// MARK: - Vtable Builders

extension MastBridge {

    /// Wraps a retained EmbedderContext and the built vtable.
    /// Call release() when the vtable is no longer needed.
    private struct RetainedEmbedderVtable {
        let vtable: MastMastEmbedderVtable
        private let retained: Unmanaged<EmbedderContext>

        init(vtable: MastMastEmbedderVtable, retained: Unmanaged<EmbedderContext>) {
            self.vtable = vtable
            self.retained = retained
        }

        func release() {
            retained.release()
        }
    }

    private struct RetainedCompactorVtable {
        let vtable: MastMastCompactorVtable
        private let retained: Unmanaged<CompactorContext>

        init(vtable: MastMastCompactorVtable, retained: Unmanaged<CompactorContext>) {
            self.vtable = vtable
            self.retained = retained
        }

        func release() {
            retained.release()
        }
    }

    private func buildEmbedderVtable(
        embed: @escaping (String) async -> [Float],
        dimensions: Int
    ) -> RetainedEmbedderVtable {
        let ctx = EmbedderContext(embed: embed, dimensions: dimensions)
        let retained = Unmanaged.passRetained(ctx)
        let rawPtr = retained.toOpaque()

        let vtable = MastMastEmbedderVtable(
            embed: { userData, textPtr, outLen, errOut -> UnsafeMutablePointer<Float>? in
                guard let userData, let textPtr, let outLen else { return nil }
                let ctx = Unmanaged<EmbedderContext>.fromOpaque(userData).takeUnretainedValue()
                let text = String(cString: textPtr)

                var result: [Float] = []
                let sem = DispatchSemaphore(value: 0)
                Task {
                    result = await ctx.embedFn(text)
                    sem.signal()
                }
                sem.wait()

                guard !result.isEmpty else {
                    if let errOut {
                        let msg = strdup("embed returned empty vector")
                        let ffiErr = UnsafeMutablePointer<MastMastFfiError>.allocate(capacity: 1)
                        ffiErr.pointee = MastMastFfiError(code: 3, message: msg)
                        errOut.pointee = ffiErr
                    }
                    return nil
                }

                outLen.pointee = result.count
                let buf = UnsafeMutablePointer<Float>.allocate(capacity: result.count)
                buf.initialize(from: &result, count: result.count)
                return buf
            },
            embed_batch: nil,
            dimensions: { userData -> Int in
                guard let userData else { return 0 }
                let ctx = Unmanaged<EmbedderContext>.fromOpaque(userData).takeUnretainedValue()
                return ctx.dims
            },
            user_data: rawPtr
        )

        return RetainedEmbedderVtable(vtable: vtable, retained: retained)
    }

    private func buildCompactorVtable(
        synthesize: @escaping ([String]) async -> String
    ) -> RetainedCompactorVtable {
        let ctx = CompactorContext(synthesize: synthesize)
        let retained = Unmanaged.passRetained(ctx)
        let rawPtr = retained.toOpaque()

        let vtable = MastMastCompactorVtable(
            synthesize: { userData, contentsPtr, count, errOut -> UnsafeMutablePointer<CChar>? in
                guard let userData, let contentsPtr, count > 0 else { return nil }
                let ctx = Unmanaged<CompactorContext>.fromOpaque(userData).takeUnretainedValue()

                var contents: [String] = []
                contents.reserveCapacity(count)
                for i in 0..<count {
                    if let cStr = contentsPtr[i] {
                        contents.append(String(cString: cStr))
                    }
                }

                var result = ""
                let sem = DispatchSemaphore(value: 0)
                Task {
                    result = await ctx.synthesizeFn(contents)
                    sem.signal()
                }
                sem.wait()

                return strdup(result)
            },
            user_data: rawPtr
        )

        return RetainedCompactorVtable(vtable: vtable, retained: retained)
    }
}
