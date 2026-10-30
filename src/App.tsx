import { type KeyboardEvent, useEffect, useRef, useState } from "react";
import { FileStatus } from "./FileStatus";
import { formatSize } from "./files";
import {
  emptySelection,
  moveSelection,
  removeSelection,
  type Selection,
  selectRow,
} from "./selection";
import { useFiles } from "./useFiles";
import "./App.css";

function App() {
  const {
    files,
    message,
    issues,
    dragging,
    busy,
    nativeError,
    chooseFiles,
    remove,
    dismissErrors,
  } = useFiles();
  const [selection, setSelection] = useState(emptySelection);
  const table = useRef<HTMLTableElement>(null);
  const addButton = useRef<HTMLButtonElement>(null);
  const paths = files.map((file) => file.path);
  const errors = [
    ...new Set([...(nativeError ? [nativeError] : []), ...issues]),
  ];
  const focusIndex = paths.indexOf(selection.focus ?? "");

  useEffect(() => {
    if (focusIndex >= 0)
      table.current
        ?.querySelectorAll("tbody tr")
        [focusIndex]?.scrollIntoView({ block: "nearest" });
  }, [focusIndex]);

  function updateSelection(next: Selection) {
    setSelection(next);
    table.current?.focus({ preventScroll: true });
  }
  function handleKey(event: KeyboardEvent<HTMLTableElement>) {
    const command = event.metaKey || event.ctrlKey;
    if (command && event.key.toLowerCase() === "a") {
      event.preventDefault();
      setSelection({
        selected: new Set(paths),
        focus: selection.focus ?? paths[0],
        anchor: selection.anchor ?? paths[0],
      });
    } else if (event.key === "ArrowUp" || event.key === "ArrowDown") {
      event.preventDefault();
      setSelection(
        moveSelection(
          selection,
          paths,
          event.key === "ArrowDown" ? 1 : -1,
          event.shiftKey,
        ),
      );
    } else if (event.key === "Delete" || event.key === "Backspace") {
      event.preventDefault();
      if (!selection.selected.size) return;
      const next = removeSelection(selection, paths);
      remove(selection.selected);
      setSelection(next);
      if (!next.focus) addButton.current?.focus();
    } else if (event.key === "Escape") {
      event.preventDefault();
      setSelection({ ...selection, selected: new Set() });
    }
  }

  return (
    <main
      className={dragging ? "app is-dragging" : "app"}
      aria-label="Zapbeam images"
    >
      {files.length ? (
        <div className="table-scroll">
          <table
            ref={table}
            // biome-ignore lint/a11y/noNoninteractiveElementToInteractiveRole: Keyboard-operated ARIA grid uses native table semantics.
            role="grid"
            aria-label="Images"
            aria-multiselectable="true"
            aria-describedby="table-help"
            aria-activedescendant={
              focusIndex >= 0 ? `file-${focusIndex}` : undefined
            }
            tabIndex={0}
            onKeyDown={handleKey}
          >
            <colgroup>
              <col className="status-column" />
              <col />
              <col className="size-column" />
              <col className="savings-column" />
            </colgroup>
            <thead>
              <tr>
                <th scope="col">
                  <span className="sr-only">Status</span>
                </th>
                <th scope="col">File</th>
                <th scope="col" className="numeric">
                  Size
                </th>
                <th scope="col" className="numeric">
                  Savings
                </th>
              </tr>
            </thead>
            <tbody>
              {files.map((file, index) => (
                <tr
                  key={file.path}
                  id={`file-${index}`}
                  aria-selected={selection.selected.has(file.path)}
                  className={
                    selection.focus === file.path ? "focused" : undefined
                  }
                  onClick={(event) =>
                    updateSelection(
                      selectRow(
                        selection,
                        paths,
                        file.path,
                        event.shiftKey,
                        event.metaKey || event.ctrlKey,
                      ),
                    )
                  }
                >
                  <td>
                    <FileStatus state="unprocessed" />
                  </td>
                  <td className="filename" title={file.name}>
                    {file.name}
                  </td>
                  <td className="numeric">{formatSize(file.size)}</td>
                  <td className="numeric savings" />
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <div
          className="drop-zone"
          role="img"
          aria-label="Drop JPEG or PNG images here, or use Add Files"
        >
          <svg
            aria-hidden="true"
            viewBox="0 0 64 64"
            fill="none"
            stroke="currentColor"
            strokeWidth="2.5"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <path d="M32 10v42M16 36l16 16 16-16" />
          </svg>
        </div>
      )}
      <p id="table-help" className="sr-only">
        Use arrow keys to select images, Shift to select a range, Command or
        Control to toggle selections, and Delete to remove entries from the
        list. Optimization is not available yet.
      </p>
      <div className="sr-only" role="status">
        {message}
      </div>
      {errors.length > 0 && (
        <aside className="errors" aria-label="Files could not be added">
          <div role="alert">
            <ul>
              {errors.map((error) => (
                <li key={error}>{error}</li>
              ))}
            </ul>
          </div>
          <button
            type="button"
            aria-label="Dismiss errors"
            title="Dismiss errors"
            onClick={() => {
              dismissErrors();
              addButton.current?.focus();
            }}
          >
            <svg aria-hidden="true" viewBox="0 0 20 20">
              <path d="m6 6 8 8m0-8-8 8" />
            </svg>
          </button>
        </aside>
      )}
      <div className="floating-actions">
        {busy && <FileStatus state="processing" label="Inspecting files" />}
        <button
          ref={addButton}
          type="button"
          className="add-files"
          aria-label="Add Files"
          aria-keyshortcuts="Meta+O Control+O"
          title="Add Files (⌘O / Ctrl+O)"
          onClick={() => void chooseFiles()}
        >
          <svg aria-hidden="true" viewBox="0 0 24 24">
            <path d="M12 5v14M5 12h14" />
          </svg>
        </button>
      </div>
    </main>
  );
}
export default App;
