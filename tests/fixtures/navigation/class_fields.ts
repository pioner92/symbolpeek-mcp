export class MessageStore {
  static addMessage = (message: string) => {
    const pendingFromDB = message;
    return pendingFromDB;
  };

  sendMessage = function (message: string) {
    const forwardBlock = message;
    return forwardBlock;
  };

  getInstance() {
    return this;
  }
}
