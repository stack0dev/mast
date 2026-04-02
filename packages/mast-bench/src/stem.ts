/**
 * Porter1 stemmer — matches NLTK's PorterStemmer for LOCOMO F1 parity.
 * Inline implementation, no external deps.
 */

function isConsonant(word: string, i: number): boolean {
  const c = word[i];
  if ("aeiou".includes(c)) return false;
  if (c === "y") return i === 0 || !isConsonant(word, i - 1);
  return true;
}

/** Count VC sequences (measure) in stem */
function measure(word: string): number {
  let n = 0;
  let i = 0;
  const len = word.length;
  while (i < len && isConsonant(word, i)) i++;
  while (i < len) {
    while (i < len && !isConsonant(word, i)) i++;
    if (i >= len) break;
    n++;
    while (i < len && isConsonant(word, i)) i++;
  }
  return n;
}

function hasVowel(word: string): boolean {
  for (let i = 0; i < word.length; i++) {
    if (!isConsonant(word, i)) return true;
  }
  return false;
}

function endsDouble(word: string): boolean {
  if (word.length < 2) return false;
  return word[word.length - 1] === word[word.length - 2] && isConsonant(word, word.length - 1);
}

/** *o — stem ends cvc where second c is not w, x, y */
function endsCVC(word: string): boolean {
  const len = word.length;
  if (len < 3) return false;
  return (
    isConsonant(word, len - 3) &&
    !isConsonant(word, len - 2) &&
    isConsonant(word, len - 1) &&
    !"wxy".includes(word[len - 1])
  );
}

function replaceSuffix(word: string, suffix: string, replacement: string): string {
  return word.slice(0, -suffix.length) + replacement;
}

function step1a(word: string): string {
  if (word.endsWith("sses")) return replaceSuffix(word, "sses", "ss");
  if (word.endsWith("ies")) return replaceSuffix(word, "ies", "i");
  if (word.endsWith("ss")) return word;
  if (word.endsWith("s")) return word.slice(0, -1);
  return word;
}

function step1b(word: string): string {
  if (word.endsWith("eed")) {
    const stem = replaceSuffix(word, "eed", "ee");
    return measure(word.slice(0, -3)) > 0 ? stem : word;
  }

  let stemmed: string | null = null;
  if (word.endsWith("ed")) {
    const stem = word.slice(0, -2);
    if (hasVowel(stem)) stemmed = stem;
  } else if (word.endsWith("ing")) {
    const stem = word.slice(0, -3);
    if (hasVowel(stem)) stemmed = stem;
  }

  if (stemmed === null) return word;

  if (stemmed.endsWith("at") || stemmed.endsWith("bl") || stemmed.endsWith("iz")) {
    return stemmed + "e";
  }
  if (endsDouble(stemmed) && !"lsz".includes(stemmed[stemmed.length - 1])) {
    return stemmed.slice(0, -1);
  }
  if (measure(stemmed) === 1 && endsCVC(stemmed)) {
    return stemmed + "e";
  }
  return stemmed;
}

function step1c(word: string): string {
  if (word.endsWith("y") && hasVowel(word.slice(0, -1))) {
    return word.slice(0, -1) + "i";
  }
  return word;
}

const step2Map: [string, string][] = [
  ["ational", "ate"], ["tional", "tion"], ["enci", "ence"], ["anci", "ance"],
  ["izer", "ize"], ["abli", "able"], ["alli", "al"], ["entli", "ent"],
  ["eli", "e"], ["ousli", "ous"], ["ization", "ize"], ["ation", "ate"],
  ["ator", "ate"], ["alism", "al"], ["iveness", "ive"], ["fulness", "ful"],
  ["ousness", "ous"], ["aliti", "al"], ["iviti", "ive"], ["biliti", "ble"],
];

function step2(word: string): string {
  for (const [suffix, replacement] of step2Map) {
    if (word.endsWith(suffix)) {
      const stem = word.slice(0, -suffix.length);
      if (measure(stem) > 0) return stem + replacement;
      return word;
    }
  }
  return word;
}

const step3Map: [string, string][] = [
  ["icate", "ic"], ["ative", ""], ["alize", "al"],
  ["iciti", "ic"], ["ical", "ic"], ["ful", ""], ["ness", ""],
];

function step3(word: string): string {
  for (const [suffix, replacement] of step3Map) {
    if (word.endsWith(suffix)) {
      const stem = word.slice(0, -suffix.length);
      if (measure(stem) > 0) return stem + replacement;
      return word;
    }
  }
  return word;
}

const step4Suffixes = [
  "al", "ance", "ence", "er", "ic", "able", "ible", "ant",
  "ement", "ment", "ent", "ion", "ou", "ism", "ate", "iti",
  "ous", "ive", "ize",
];

function step4(word: string): string {
  for (const suffix of step4Suffixes) {
    if (word.endsWith(suffix)) {
      const stem = word.slice(0, -suffix.length);
      if (suffix === "ion") {
        if (stem.length > 0 && (stem.endsWith("s") || stem.endsWith("t")) && measure(stem) > 1) {
          return stem;
        }
      } else if (measure(stem) > 1) {
        return stem;
      }
    }
  }
  return word;
}

function step5a(word: string): string {
  if (word.endsWith("e")) {
    const stem = word.slice(0, -1);
    if (measure(stem) > 1) return stem;
    if (measure(stem) === 1 && !endsCVC(stem)) return stem;
  }
  return word;
}

function step5b(word: string): string {
  if (endsDouble(word) && word.endsWith("l") && measure(word) > 1) {
    return word.slice(0, -1);
  }
  return word;
}

export function porterStem(word: string): string {
  if (word.length <= 2) return word;
  let w = word.toLowerCase();
  w = step1a(w);
  w = step1b(w);
  w = step1c(w);
  w = step2(w);
  w = step3(w);
  w = step4(w);
  w = step5a(w);
  w = step5b(w);
  return w;
}
