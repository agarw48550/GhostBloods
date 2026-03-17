// GhostBloods — Alert Scorer
// Simple, explainable scoring: severity keywords + source weight + recency + watchlist match

import { createHash } from 'crypto';

// === Severity Keyword Banks ===
const SEVERITY_KEYWORDS = {
  critical: {
    words: ['nuclear', 'nuke', 'icbm', 'missile launch', 'invasion', 'declaration of war',
            'martial law', 'coup', 'flash crash', 'bank run', 'meltdown', 'radiation leak'],
    weight: 10,
  },
  high: {
    words: ['missile', 'airstrike', 'bombing', 'casualties', 'killed', 'troops deployed',
            'sanctions', 'blockade', 'ceasefire collapse', 'emergency', 'war crime',
            'escalation', 'mobilization', 'cyber attack', 'blackout'],
    weight: 6,
  },
  elevated: {
    words: ['conflict', 'military', 'protest', 'riot', 'explosion', 'shooting',
            'tariff', 'trade war', 'default', 'recession', 'crisis', 'warning',
            'tension', 'threat', 'deployment', 'exercise', 'drill'],
    weight: 3,
  },
  moderate: {
    words: ['diplomatic', 'negotiation', 'summit', 'election', 'referendum',
            'inflation', 'unemployment', 'supply chain', 'embargo'],
    weight: 1,
  },
};

// === Content Hash for Dedup ===
function contentHash(text) {
  if (!text) return '';
  const normalized = text
    .toLowerCase()
    .replace(/\d{1,2}:\d{2}(:\d{2})?/g, '')
    .replace(/\d+/g, 'N')
    .replace(/[^\w\s]/g, '')
    .replace(/\s+/g, ' ')
    .trim()
    .substring(0, 100);
  return createHash('sha256').update(normalized).digest('hex').substring(0, 12);
}

// === Recency Score ===
function recencyScore(dateStr) {
  if (!dateStr) return 0;
  try {
    const age = Date.now() - new Date(dateStr).getTime();
    const hours = age / (1000 * 60 * 60);
    if (hours < 1) return 3;
    if (hours < 6) return 1;
    return 0;
  } catch {
    return 0;
  }
}

// === Main Scoring Function ===
export function scoreArticles(articles, options = {}) {
  const {
    watchlistKeywords = [],
    watchlistRegions = [],
    threshold = 8,
    existingHashes = new Set(),
  } = options;

  const scored = [];

  for (const article of articles) {
    const title = (article.title || '').toLowerCase();
    const hash = contentHash(article.title);

    // Skip duplicates
    if (existingHashes.has(hash)) continue;

    let score = 0;
    let matchedKeywords = [];

    // 1. Severity keyword matching
    for (const [level, config] of Object.entries(SEVERITY_KEYWORDS)) {
      for (const word of config.words) {
        if (title.includes(word.toLowerCase())) {
          score += config.weight;
          matchedKeywords.push(`${level}:${word}`);
        }
      }
    }

    // 2. Source weight
    score += article.sourceWeight || 1;

    // 3. Recency boost
    score += recencyScore(article.date);

    // 4. Watchlist keyword match
    for (const keyword of watchlistKeywords) {
      if (title.includes(keyword.toLowerCase())) {
        score += 5;
        matchedKeywords.push(`watchlist:${keyword}`);
      }
    }

    // 5. Watchlist region match
    for (const region of watchlistRegions) {
      if (title.includes(region.toLowerCase())) {
        score += 3;
        matchedKeywords.push(`region:${region}`);
      }
    }

    // Only include if above threshold
    if (score >= threshold) {
      // Determine tier
      let tier = 'ROUTINE';
      if (score >= 15) tier = 'FLASH';
      else if (score >= threshold) tier = 'PRIORITY';

      scored.push({
        title: article.title,
        url: article.url,
        source: article.source,
        date: article.date,
        score,
        tier,
        hash,
        keywords: matchedKeywords.slice(0, 5),
      });
    }
  }

  // Sort by score descending
  scored.sort((a, b) => b.score - a.score);

  // Cap at 10 alerts per sweep
  return scored.slice(0, 10);
}
