export interface ImageFile {
  path: string;
  name: string;
  format: "JPEG" | "PNG";
  size: number;
}
export type Inspection =
  | { kind: "image"; file: ImageFile }
  | { kind: "rejected"; name: string; message: string };

export function mergeFiles(current: ImageFile[], results: Inspection[]) {
  const paths = new Set(current.map((file) => file.path));
  const files = [...current];
  const issues: string[] = [];
  let duplicates = 0;
  for (const result of results) {
    if (result.kind === "rejected") {
      issues.push(`${result.name}: ${result.message}`);
    } else if (paths.has(result.file.path)) {
      duplicates++;
    } else {
      paths.add(result.file.path);
      files.push(result.file);
    }
  }
  return { files, issues, duplicates, added: files.length - current.length };
}

export function formatSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "—";
  if (bytes < 1000) return `${bytes} B`;
  const units = ["kB", "MB", "GB", "TB"];
  let value = bytes / 1000;
  let unit = 0;
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000;
    unit++;
  }
  return `${value.toLocaleString("en-US", { maximumFractionDigits: 1 })} ${units[unit]}`;
}
