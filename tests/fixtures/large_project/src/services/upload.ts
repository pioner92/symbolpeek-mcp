import { sendMessage } from './send';

export function enqueueUpload(message: string) {
  return { message, retry: () => sendMessage(message) };
}

