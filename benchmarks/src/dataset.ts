import { DATASET_SIZE, NUM_TOPICS, NUM_QUERIES } from "./config.js";

export interface Document {
  id: string;
  text: string;
  topic: number;
}

export interface Query {
  text: string;
  relevantIds: string[];
}

export interface Dataset {
  documents: Document[];
  queries: Query[];
}

// Simple seeded PRNG (mulberry32)
function mulberry32(seed: number) {
  return () => {
    let t = (seed += 0x6d2b79f5);
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const TOPIC_NAMES = [
  "machine learning",
  "distributed systems",
  "database optimization",
  "network security",
  "cloud computing",
  "web development",
  "mobile applications",
  "data engineering",
  "DevOps practices",
  "API design",
];

const TOPIC_SENTENCES = [
  [
    "Neural networks learn hierarchical representations from data",
    "Gradient descent optimizes model parameters iteratively",
    "Feature engineering improves model accuracy significantly",
    "Cross-validation prevents overfitting on training data",
    "Transfer learning reuses knowledge across related tasks",
  ],
  [
    "Consensus protocols ensure data consistency across nodes",
    "Sharding distributes workload across multiple servers",
    "Leader election maintains system coordination automatically",
    "Replication provides fault tolerance in distributed clusters",
    "Message queues decouple services for better scalability",
  ],
  [
    "Index selection dramatically improves query performance",
    "Query planners choose optimal execution strategies",
    "Partitioning enables parallel processing of large tables",
    "Materialized views cache expensive computation results",
    "Connection pooling reduces database overhead significantly",
  ],
  [
    "Encryption protects data in transit and at rest",
    "Firewalls filter malicious traffic at network boundaries",
    "Authentication verifies user identity before granting access",
    "Intrusion detection systems monitor for suspicious activity",
    "Zero trust architecture never implicitly trusts any request",
  ],
  [
    "Auto-scaling adjusts capacity based on current demand",
    "Containers provide consistent deployment environments everywhere",
    "Serverless functions execute code without managing infrastructure",
    "Load balancers distribute traffic across healthy instances",
    "CDNs cache content closer to end users globally",
  ],
  [
    "Single-page applications render content dynamically client-side",
    "Progressive web apps work offline with service workers",
    "Server-side rendering improves initial page load performance",
    "Component-based architecture promotes code reuse and maintainability",
    "WebSocket connections enable real-time bidirectional communication",
  ],
  [
    "Native apps leverage platform-specific APIs for performance",
    "Cross-platform frameworks share code across mobile platforms",
    "Push notifications engage users with timely updates",
    "Offline-first design ensures functionality without connectivity",
    "Gesture recognition creates intuitive touch-based interfaces",
  ],
  [
    "ETL pipelines transform raw data into analytical formats",
    "Stream processing handles continuous data flows in real-time",
    "Data lakes store unstructured data for future analysis",
    "Schema evolution manages changing data structures gracefully",
    "Data lineage tracks transformations from source to destination",
  ],
  [
    "Infrastructure as code automates environment provisioning",
    "Continuous integration detects bugs early in development",
    "Monitoring and alerting ensure system reliability proactively",
    "Blue-green deployments minimize downtime during releases",
    "Chaos engineering validates system resilience under failure",
  ],
  [
    "REST conventions provide predictable resource-oriented endpoints",
    "GraphQL enables clients to request exactly needed data",
    "Rate limiting protects APIs from excessive request volumes",
    "Versioning strategies maintain backward compatibility over time",
    "OpenAPI specifications document API contracts automatically",
  ],
];

export function generateDataset(): Dataset {
  const rng = mulberry32(42);
  const docsPerTopic = DATASET_SIZE / NUM_TOPICS;
  const documents: Document[] = [];

  for (let topic = 0; topic < NUM_TOPICS; topic++) {
    const sentences = TOPIC_SENTENCES[topic];
    for (let i = 0; i < docsPerTopic; i++) {
      const sentenceIdx = Math.floor(rng() * sentences.length);
      const detail = `Detail ${i}: ${TOPIC_NAMES[topic]} involves complex considerations around scalability, correctness, and performance trade-offs in practice. Instance ${Math.floor(rng() * 10000)}.`;
      documents.push({
        id: `doc-${topic}-${i}`,
        text: `Topic ${topic} (${TOPIC_NAMES[topic]}): ${sentences[sentenceIdx]}. ${detail}`,
        topic,
      });
    }
  }

  // Generate queries: 5 per topic
  const queriesPerTopic = NUM_QUERIES / NUM_TOPICS;
  const queries: Query[] = [];

  for (let topic = 0; topic < NUM_TOPICS; topic++) {
    const topicDocs = documents
      .filter((d) => d.topic === topic)
      .map((d) => d.id);

    for (let q = 0; q < queriesPerTopic; q++) {
      const sentences = TOPIC_SENTENCES[topic];
      const sentenceIdx = Math.floor(rng() * sentences.length);
      queries.push({
        text: `Tell me about ${TOPIC_NAMES[topic]}. ${sentences[sentenceIdx]}`,
        relevantIds: topicDocs,
      });
    }
  }

  return { documents, queries };
}
