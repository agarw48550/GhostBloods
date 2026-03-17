#!/usr/bin/env node
// GhostBloods — Lightweight background sweep
// Queries GDELT + RSS feeds, scores alerts, outputs JSON to stdout, then exits.
// Designed to run as a one-shot child process from the Tauri notifier.

import { readFileSync } from 'fs';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { scoreArticles } from './scorer.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const FEEDS_PATH = join(__dirname, 'rss-feeds.json');
const MAX_RSS_ITEMS_PER_FEED = 5;
const GDELT_MAX_RECORDS = 25;
const GDELT_TIMEOUT_MS = 12000;
const RSS_TIMEOUT_MS = 8000;

// Read state from env (passed by Rust notifier)
let state = {};
try {
  state = JSON.parse(process.env.GHOSTBLOODS_STATE || '{}');
} catch { /* use defaults */ }

const settings = state.settings || {};
const existingHashes = new Set(state.alert_hashes || []);
const watchlistKeywords = settings.watchlist_keywords || ['nuclear', 'missile', 'invasion', 'sanctions'];
const watchlistRegions = settings.watchlist_regions || ['Ukraine', 'Taiwan', 'Middle East'];
const threshold = settings.threshold || 8;

const startTime = Date.now();

// === GDELT Queries ===

async function fetchGDELT() {
  const queries = [
    'conflict OR military OR war OR missile OR nuclear',
    'sanctions OR tariff OR crisis OR protest',
  ];

  const articles = [];

  for (const q of queries) {
    try {
      const params = new URLSearchParams({
        query: q,
        mode: 'ArtList',
        maxrecords: String(GDELT_MAX_RECORDS),
        timespan: '12h',
        format: 'json',
        sort: 'DateDesc',
      });

      const res = await fetch(`https://api.gdeltproject.org/api/v2/doc/doc?${params}`, {
        signal: AbortSignal.timeout(GDELT_TIMEOUT_MS),
      });

      if (res.ok) {
        const data = await res.json();
        for (const a of (data.articles || [])) {
          articles.push({
            title: a.title || '',
            url: a.url || '',
            source: `GDELT:${a.domain || 'unknown'}`,
            date: a.seendate || '',
            sourceWeight: 3,
          });
        }
      }
    } catch (err) {
      // Non-fatal: GDELT may be down or rate-limited
      console.error(`[lite-sweep] GDELT query failed: ${err.message}`);
    }

    // GDELT rate limit: 5s between requests
    await new Promise(r => setTimeout(r, 5500));
  }

  return articles;
}

// === RSS Feed Fetching ===

function parseRSSItems(xml, feedName) {
  const items = [];
  // Simple XML parsing without dependencies
  const itemRegex = /<item>([\s\S]*?)<\/item>/gi;
  let match;
  let count = 0;

  while ((match = itemRegex.exec(xml)) !== null && count < MAX_RSS_ITEMS_PER_FEED) {
    const itemXml = match[1];
    const title = extractTag(itemXml, 'title');
    const link = extractTag(itemXml, 'link');
    const pubDate = extractTag(itemXml, 'pubDate');

    if (title) {
      items.push({
        title: cleanText(title),
        url: link || '',
        source: `RSS:${feedName}`,
        date: pubDate || '',
        sourceWeight: feedName.includes('Reuters') || feedName.includes('AP') ? 3 : 
                      feedName.includes('BBC') || feedName.includes('Al Jazeera') ? 3 : 1,
      });
      count++;
    }
  }

  // Also try Atom format
  if (items.length === 0) {
    const entryRegex = /<entry>([\s\S]*?)<\/entry>/gi;
    while ((match = entryRegex.exec(xml)) !== null && count < MAX_RSS_ITEMS_PER_FEED) {
      const entryXml = match[1];
      const title = extractTag(entryXml, 'title');
      const linkMatch = entryXml.match(/href="([^"]+)"/);
      const updated = extractTag(entryXml, 'updated') || extractTag(entryXml, 'published');

      if (title) {
        items.push({
          title: cleanText(title),
          url: linkMatch?.[1] || '',
          source: `RSS:${feedName}`,
          date: updated || '',
          sourceWeight: 2,
        });
        count++;
      }
    }
  }

  return items;
}

function extractTag(xml, tag) {
  const match = xml.match(new RegExp(`<${tag}[^>]*>\\s*(?:<!\\[CDATA\\[)?\\s*(.*?)\\s*(?:\\]\\]>)?\\s*</${tag}>`, 's'));
  return match ? match[1].trim() : '';
}

function cleanText(text) {
  return text
    .replace(/<[^>]+>/g, '')
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/\s+/g, ' ')
    .trim();
}

async function fetchRSSFeeds() {
  let feeds = [];
  try {
    feeds = JSON.parse(readFileSync(FEEDS_PATH, 'utf8'));
  } catch {
    console.error('[lite-sweep] Could not load rss-feeds.json');
    return [];
  }

  const articles = [];
  const results = await Promise.allSettled(
    feeds.map(async (feed) => {
      try {
        const res = await fetch(feed.url, {
          signal: AbortSignal.timeout(RSS_TIMEOUT_MS),
          headers: { 'User-Agent': 'GhostBloods/1.0 (OSINT Monitor)' },
        });
        if (res.ok) {
          const xml = await res.text();
          return parseRSSItems(xml, feed.name);
        }
      } catch {
        // Feed unavailable — non-fatal
      }
      return [];
    })
  );

  for (const result of results) {
    if (result.status === 'fulfilled') {
      articles.push(...result.value);
    }
  }

  return articles;
}

// === Main ===

async function main() {
  const [gdeltArticles, rssArticles] = await Promise.all([
    fetchGDELT(),
    fetchRSSFeeds(),
  ]);

  const allArticles = [...gdeltArticles, ...rssArticles];

  // Score and filter
  const scored = scoreArticles(allArticles, {
    watchlistKeywords,
    watchlistRegions,
    threshold,
    existingHashes,
  });

  const output = {
    alerts: scored,
    stats: {
      total_items: allArticles.length,
      sources_checked: 2, // GDELT + RSS
      duration_ms: Date.now() - startTime,
      gdelt_count: gdeltArticles.length,
      rss_count: rssArticles.length,
    },
  };

  // Output JSON to stdout for the Rust notifier to read
  process.stdout.write(JSON.stringify(output));
  process.exit(0);
}

main().catch(err => {
  console.error('[lite-sweep] Fatal:', err.message);
  process.stdout.write(JSON.stringify({ alerts: [], stats: { total_items: 0, sources_checked: 0, duration_ms: 0 } }));
  process.exit(1);
});
