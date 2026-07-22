export function onSuccess() {
  return "outer";
}

export const handlers = {
  onSuccess: () => onSuccess(),
  onSuccess: () => onSuccess(),
};
