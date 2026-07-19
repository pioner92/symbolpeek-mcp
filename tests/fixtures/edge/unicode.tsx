// Keep this emoji and spacing exactly as written: 🧪
export const café = (message: string) => {
  // A partially nested expression is still valid TSX.
  return <span data-label="café">{message}</span>;
};

export default () => <section>{café('hello')}</section>;

