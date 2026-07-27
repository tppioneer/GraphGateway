/**
 * Sample entry-point module for GitNexus indexing.
 * Used by the mcp-proxy / GitNexus interop verification suite.
 */

/**
 * Greet a person by name.
 * @param {string} name
 * @returns {string}
 */
function greet(name) {
  const trimmed = name.trim();
  return `Hello, ${trimmed}!`;
}

/**
 * Compute the sum of two numbers.
 * @param {number} a
 * @param {number} b
 * @returns {number}
 */
function calculateSum(a, b) {
  if (typeof a !== 'number' || typeof b !== 'number') {
    throw new TypeError('Both arguments must be numbers');
  }
  return a + b;
}

/**
 * Check whether a string is a valid email address (simple heuristic).
 * @param {string} email
 * @returns {boolean}
 */
function isValidEmail(email) {
  if (typeof email !== 'string') return false;
  return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email);
}

/**
 * Async helper that resolves after a delay.
 * @param {number} ms
 * @returns {Promise<void>}
 */
function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

module.exports = { greet, calculateSum, isValidEmail, delay };
