/**
 * Utility functions – used by main.js and independently testable.
 */

/**
 * Format a Date as ISO-8601 date-only string.
 * @param {Date} date
 * @returns {string}
 */
function formatDate(date) {
  return date.toISOString().split('T')[0];
}

/**
 * Safely parse a JSON string with a fallback.
 * @param {string} str
 * @param {*} fallback
 * @returns {*}
 */
function parseJson(str, fallback = null) {
  try {
    return JSON.parse(str);
  } catch {
    return fallback;
  }
}

/**
 * Deep-clone a JSON-compatible object.
 * @template T
 * @param {T} obj
 * @returns {T}
 */
function deepClone(obj) {
  return JSON.parse(JSON.stringify(obj));
}

/**
 * Sleep for a given number of milliseconds (async).
 * @param {number} ms
 * @returns {Promise<void>}
 */
function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * Compute nth Fibonacci number (iterative).
 * @param {number} n
 * @returns {number}
 */
function fibonacci(n) {
  if (n < 0) throw new RangeError('n must be >= 0');
  if (n <= 1) return n;
  let a = 0, b = 1;
  for (let i = 2; i <= n; i++) {
    [a, b] = [b, a + b];
  }
  return b;
}

module.exports = { formatDate, parseJson, deepClone, sleep, fibonacci };
