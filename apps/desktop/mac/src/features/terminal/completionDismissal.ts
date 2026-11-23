type EventContainer<T> = {
  contains(target: T): boolean;
};

/** Close only when the event target is outside every protected region. */
export function shouldDismissCompletion<T>(
  target: T | null,
  protectedRegions: Array<EventContainer<T> | null>,
): boolean {
  return target === null || !protectedRegions.some((region) => region?.contains(target));
}
