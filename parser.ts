export interface DiffHunk {
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
  content: string[];
}

export interface FileChange {
  path: string;
  oldPath?: string;
  status: "added" | "modified" | "deleted" | "renamed";
  hunks: DiffHunk[];
  additions: number;
  deletions: number;
  binary?: boolean;
}

const DIFF_HEADER_REGEX = /^diff --git a\/(.+?) b\/(.+?)$/m;
const NEW_FILE_REGEX = /^new file mode/m;
const DELETED_FILE_REGEX = /^deleted file mode/m;
const RENAME_FROM_REGEX = /^rename from (.+)$/m;
const RENAME_TO_REGEX = /^rename to (.+)$/m;
const BINARY_REGEX = /^Binary files/m;
const HUNK_REGEX = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/;

const parseHunk = (lines: string[]): DiffHunk | null => {
  if (lines.length === 0) return null;
  
  const hunkHeader = lines[0];
  const match = hunkHeader.match(HUNK_REGEX);
  if (!match) return null;

  const oldStart = parseInt(match[1], 10);
  const oldLines = match[2] ? parseInt(match[2], 10) : 1;
  const newStart = parseInt(match[3], 10);
  const newLines = match[4] ? parseInt(match[4], 10) : 1;

  return {
    oldStart,
    oldLines,
    newStart,
    newLines,
    content: lines.slice(1),
  };
};

export const parseDiff = (diff: string): FileChange[] => {
  if (!diff.trim()) {
    return [];
  }

  const result: FileChange[] = [];
  const fileBlocks = diff.split(/(?=^diff --git )/m).filter(Boolean);

  for (const block of fileBlocks) {
    const lines = block.split("\n");
    const headerLine = lines[0] || "";
    
    let path = "";
    let status: FileChange["status"] = "modified";
    let oldPath: string | undefined;
    let binary = false;
    let hunks: DiffHunk[] = [];
    let additions = 0;
    let deletions = 0;

    const pathMatch = headerLine.match(DIFF_HEADER_REGEX);
    if (pathMatch) {
      path = pathMatch[2];
    }

    for (const line of lines) {
      if (NEW_FILE_REGEX.test(line)) {
        status = "added";
      } else if (DELETED_FILE_REGEX.test(line)) {
        status = "deleted";
      } else if (RENAME_FROM_REGEX.test(line)) {
        status = "renamed";
        oldPath = line.match(RENAME_FROM_REGEX)?.[1];
      } else if (RENAME_TO_REGEX.test(line)) {
        path = line.match(RENAME_TO_REGEX)?.[1] || path;
      } else if (BINARY_REGEX.test(line)) {
        binary = true;
      }
    }

    if (!binary) {
      let currentHunkLines: string[] = [];
      let inHunk = false;

      for (const line of lines) {
        if (HUNK_REGEX.test(line)) {
          if (currentHunkLines.length > 0) {
            const hunk = parseHunk(currentHunkLines);
            if (hunk) hunks.push(hunk);
          }
          currentHunkLines = [line];
          inHunk = true;
        } else if (inHunk) {
          currentHunkLines.push(line);
        }
      }

      if (currentHunkLines.length > 0) {
        const hunk = parseHunk(currentHunkLines);
        if (hunk) hunks.push(hunk);
      }

      for (const hunk of hunks) {
        for (const line of hunk.content) {
          if (line.startsWith("+") && !line.startsWith("+++")) {
            additions++;
          } else if (line.startsWith("-") && !line.startsWith("---")) {
            deletions++;
          }
        }
      }
    }

    result.push({
      path,
      oldPath,
      status,
      hunks,
      additions,
      deletions,
      binary,
    });
  }

  return result;
};
