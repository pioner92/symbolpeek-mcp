declare function useMutation(
  mutation: () => void,
  options: {onSuccess: () => void},
): {mutate: () => void; isLoading: boolean};

declare function createEvent(): void;
declare function editEvent(): void;

export function EventCreation() {
  const {mutate: onCreateEvent, isLoading: isCreateLoading} = useMutation(
    createEvent,
    {
      onSuccess: () => {
        console.log("created");
      },
    },
  );
  const {mutate: onEditEvent, isLoading: isEditLoading} = useMutation(
    editEvent,
    {
      onSuccess: () => {
        console.log("edited");
      },
    },
  );
  return {onCreateEvent, isCreateLoading, onEditEvent, isEditLoading};
}
