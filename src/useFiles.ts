import { invoke, isTauri } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useRef, useState } from "react";
import { type ImageFile, type Inspection, mergeFiles } from "./files";

export function useFiles() {
  const [files, setFiles] = useState<ImageFile[]>([]);
  const queue = useRef<ImageFile[]>([]);
  const [message, setMessage] = useState("");
  const [issues, setIssues] = useState<string[]>([]);
  const [dragging, setDragging] = useState(false);
  const [busy, setBusy] = useState(false);
  const [nativeError, setNativeError] = useState("");
  const pending = useRef(Promise.resolve());
  const pickerOpen = useRef(false);
  const mounted = useRef(false);

  const ingest = useCallback((paths: string[]) => {
    pending.current = pending.current.then(async () => {
      if (!mounted.current) return;
      setBusy(true);
      setMessage("Reading image information…");
      setIssues([]);
      try {
        const results: Inspection[] = [];
        const unique = [...new Set(paths)];
        for (let offset = 0; offset < unique.length; offset += 256) {
          if (!mounted.current) return;
          results.push(
            ...(await invoke<Inspection[]>("inspect_files", {
              paths: unique.slice(offset, offset + 256),
            })),
          );
        }
        if (!mounted.current) return;
        const result = mergeFiles(queue.current, results);
        queue.current = result.files;
        setFiles(result.files);
        setIssues(result.issues);
        const duplicates = result.duplicates + paths.length - unique.length;
        setMessage(
          `${result.added} ${result.added === 1 ? "image" : "images"} added.${duplicates ? ` ${duplicates} already in the list.` : ""}${result.issues.length ? ` ${result.issues.length} could not be added.` : ""}`,
        );
      } catch {
        if (mounted.current)
          setIssues(["Could not read these files. Try adding them again."]);
      } finally {
        if (mounted.current) setBusy(false);
      }
    });
  }, []);

  const chooseFiles = useCallback(async () => {
    if (pickerOpen.current) return;
    pickerOpen.current = true;
    try {
      const paths = await open({
        multiple: true,
        directory: false,
        title: "Add images",
        filters: [
          { name: "JPEG and PNG images", extensions: ["jpg", "jpeg", "png"] },
        ],
      });
      if (paths && mounted.current) ingest(paths);
    } catch {
      if (mounted.current)
        setIssues([
          "Could not open the file picker. Try dropping images into the window.",
        ]);
    } finally {
      pickerOpen.current = false;
    }
  }, [ingest]);

  useEffect(() => {
    mounted.current = true;
    let active = true;
    let unlisten: (() => void) | undefined;
    if (!isTauri())
      setNativeError("Open Zapbeam in its desktop window to add files.");
    else {
      invoke("backend_status").catch(() => {
        if (active)
          setNativeError(
            "Could not connect to Zapbeam. Restart the app to try again.",
          );
      });
      getCurrentWebview()
        .onDragDropEvent(({ payload }) => {
          if (!active) return;
          setDragging(payload.type === "enter" || payload.type === "over");
          if (payload.type === "drop") ingest(payload.paths);
        })
        .then((cleanup) => {
          if (active) unlisten = cleanup;
          else cleanup();
        })
        .catch(() => {
          if (active)
            setNativeError(
              "Drag and drop is unavailable. Use Add Files or restart the app.",
            );
        });
    }
    const keydown = (event: KeyboardEvent) => {
      if (
        (event.metaKey || event.ctrlKey) &&
        event.key.toLowerCase() === "o" &&
        !event.altKey
      ) {
        event.preventDefault();
        if (!event.repeat) void chooseFiles();
      }
    };
    window.addEventListener("keydown", keydown);
    return () => {
      active = false;
      mounted.current = false;
      unlisten?.();
      window.removeEventListener("keydown", keydown);
    };
  }, [chooseFiles, ingest]);

  function remove(paths: Set<string>) {
    queue.current = queue.current.filter((file) => !paths.has(file.path));
    setFiles(queue.current);
    setMessage(`${paths.size} entries removed from the list.`);
  }
  function dismissErrors() {
    setIssues([]);
    setNativeError("");
  }
  return {
    files,
    message,
    issues,
    dragging,
    busy,
    nativeError,
    chooseFiles,
    remove,
    dismissErrors,
  };
}
