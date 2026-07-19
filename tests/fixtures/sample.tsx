export interface Message {
  body: string;
}

const MAX_LENGTH = 200;

function validateInput(message: Message) {
  return message.body.length <= MAX_LENGTH;
}

export async function sendMessage(message: Message) {
  function normalize() {
    return message.body.trim();
  }
  if (!validateInput(message)) return;
  return normalize();
}

export const MessageList = ({ messages }: { messages: Message[] }) => (
  <ul>{messages.map((message) => <li>{message.body}</li>)}</ul>
);

export function* messages() {
  yield* [];
}

class MessageStore {
  append(message: Message) {
    return message;
  }
}

const api = {
  enqueueUpload(file: string) {
    return file;
  },
  send: (message: Message) => sendMessage(message),
};

