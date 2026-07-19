const helper = (value) => value.trim();

export function sendMessage(value) {
  return helper(value);
}

export default function createClient() {
  return { sendMessage };
}

module.exports = { sendMessage, createClient };
exports.helper = helper;

