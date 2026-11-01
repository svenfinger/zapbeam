export type FileState =
  | "unprocessed"
  | "processing"
  | "compressed"
  | "unchanged"
  | "error";
const labels: Record<FileState, string> = {
  unprocessed: "Queued for optimization",
  processing: "Processing",
  compressed: "Compressed",
  unchanged: "Unchanged — no smaller valid image produced",
  error: "Processing failed — original unchanged",
};

export function FileStatus({
  state,
  label = labels[state],
}: {
  state: FileState;
  label?: string;
}) {
  return (
    <span
      className={`file-status ${state}`}
      role="img"
      aria-label={label}
      title={label}
    >
      <svg
        aria-hidden="true"
        viewBox="0 0 20 20"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        {state === "processing" ? (
          <path d="M10 2a8 8 0 1 1-8 8" />
        ) : (
          <circle cx="10" cy="10" r="7.5" />
        )}
        {state === "compressed" && <path d="m6.5 10 2.3 2.3 4.7-4.6" />}
        {state === "unchanged" && <path d="M6.5 10h7" />}
        {state === "error" && <path d="M10 6v5m0 3h.01" />}
      </svg>
    </span>
  );
}
