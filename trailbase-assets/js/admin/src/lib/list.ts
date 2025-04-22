export type ListArgs = {
  filter: string | undefined | null;

  pageSize: number;
  pageIndex: number;

  cursor: string | undefined | null;
  prevCursors: string[];
};

export function buildListSearchParams({
  filter,
  pageSize,
  pageIndex,
  cursor,
  prevCursors,
}: ListArgs): URLSearchParams {
  // TODO: Avoid dis- and then re-assembling filter params here.
  // TODO: support OR.
  // TODO: parse more loosely, e.g. AND vs and vs &&.
  const filterParams = filter
    ?.split(/AND/)
    .map((frag: string) => frag.trim())
    .join("&");
  console.log("PARAMS", filterParams);
  const params = new URLSearchParams(filterParams);

  params.set("limit", pageSize.toString());

  // Build the next cursor from previous response and update local
  // cursor stack. If we're paging forward we add new cursors, otherwise we're
  // re-using previously seen cursors for consistency. We reset if we go back
  // to the start.
  if (pageIndex === 0) {
    prevCursors.length = 0;
  } else {
    const index = pageIndex - 1;
    if (index < prevCursors.length) {
      // Already known page
      params.set("cursor", prevCursors[index]);
    } else {
      // New page case: use cursor from previous response or fall back to more
      // expensive and inconsistent offset-based pagination.
      if (cursor) {
        prevCursors.push(cursor);
        params.set("cursor", cursor);
      } else {
        params.set("offset", `${pageIndex * pageSize}`);
      }
    }
  }

  return params;
}
