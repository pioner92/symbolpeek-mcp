import { enqueueUpload } from './upload';
import { formatMessage } from '../utils/format';

export async function sendMessage(message: string) {
  return enqueueUpload(formatMessage(message));
}

