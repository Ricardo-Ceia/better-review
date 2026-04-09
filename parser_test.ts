import { assertEquals, assertExists } from "@std/assert";
import { parseDiff } from "./parser.ts";

const emptyDiff = "";

const addedFileDiff = `diff --git a/new.txt b/new.txt
new file mode 100644
index 0000000..abc1234
--- /dev/null
+++ b/new.txt
@@ -0,0 +1,2 @@
+new content line 1
+new content line 2
`;

const modifiedFileDiff = `diff --git a/modified.txt b/modified.txt
index abc1234..def5678 100644
--- a/modified.txt
+++ b/modified.txt
@@ -1,3 +1,4 @@
 line 1
 line 2
+added line
 line 3
`;

const deletedFileDiff = `diff --git a/deleted.txt b/deleted.txt
deleted file mode 100644
index abc1234..0000000
--- a/deleted.txt
+++ /dev/null
@@ -1,2 +0,0 @@
-old content line 1
-old content line 2
`;

const renamedFileDiff = `diff --git a/old.txt b/new.txt
rename from old.txt
rename to new.txt
index abc1234..def5678 100644
--- a/old.txt
+++ b/new.txt
@@ -1,2 +1,2 @@
-old content
+new content
`;

const binaryFileDiff = `diff --git a/image.jpg b/image.jpg
Binary files a/image.jpg b/image.jpg differ
`;

const multipleFilesDiff = `diff --git a/file1.txt b/file1.txt
new file mode 100644
index 0000000..abc1234
--- /dev/null
+++ b/file1.txt
@@ -0,0 +1 @@
+file1 content
diff --git a/file2.txt b/file2.txt
index abc1234..def5678 100644
--- a/file2.txt
+++ b/file2.txt
@@ -1 +1 @@
-old
+new
diff --git a/file3.txt b/file3.txt
deleted file mode 100644
index abc1234..0000000
--- a/file3.txt
+++ /dev/null
@@ -1 +0,0 @@
-file3
`;

const noNewlineDiff = `diff --git a/noeof.txt b/noeof.txt
index abc1234..def5678 100644
--- a/noeof.txt
+++ b/noeof.txt
@@ -1,2 +1,2 @@
 line 1
-line 2
\ No newline at end of file
+line 2
`;

const largeHunkDiff = `diff --git a/large.txt b/large.txt
index abc1234..def5678 100644
--- a/large.txt
+++ b/large.txt
@@ -1,10 +1,10 @@
 line 1
 line 2
 line 3
 line 4
 line 5
 line 6
 line 7
 line 8
 line 9
-line 10
+line 10 modified
`;

const unmergedFileDiff = `diff --git a/conflicted.txt b/conflicted.txt
index abc1234..def5678 100644
--- a/conflicted.txt
+++ b/conflicted.txt
@@ -1 +1 @@
-<<<<<<<
+=======
-old
+new
>>>>>>>
`;

Deno.test("parseDiff: empty string returns empty array", () => {
  const result = parseDiff(emptyDiff);
  assertEquals(result.length, 0);
});

Deno.test("parseDiff: added file returns correct status and counts", () => {
  const result = parseDiff(addedFileDiff);
  assertEquals(result.length, 1);
  assertEquals(result[0].path, "new.txt");
  assertEquals(result[0].status, "added");
  assertEquals(result[0].additions, 2);
  assertEquals(result[0].deletions, 0);
  assertExists(result[0].hunks);
  assertEquals(result[0].hunks.length, 1);
});

Deno.test("parseDiff: modified file returns correct status and counts", () => {
  const result = parseDiff(modifiedFileDiff);
  assertEquals(result.length, 1);
  assertEquals(result[0].path, "modified.txt");
  assertEquals(result[0].status, "modified");
  assertEquals(result[0].additions, 1);
  assertEquals(result[0].deletions, 0);
});

Deno.test("parseDiff: deleted file returns deleted status", () => {
  const result = parseDiff(deletedFileDiff);
  assertEquals(result.length, 1);
  assertEquals(result[0].path, "deleted.txt");
  assertEquals(result[0].status, "deleted");
  assertEquals(result[0].additions, 0);
  assertEquals(result[0].deletions, 2);
});

Deno.test("parseDiff: renamed file preserves oldPath", () => {
  const result = parseDiff(renamedFileDiff);
  assertEquals(result.length, 1);
  assertEquals(result[0].path, "new.txt");
  assertEquals(result[0].oldPath, "old.txt");
  assertEquals(result[0].status, "renamed");
});

Deno.test("parseDiff: binary file detected", () => {
  const result = parseDiff(binaryFileDiff);
  assertEquals(result.length, 1);
  assertEquals(result[0].path, "image.jpg");
  assertEquals(result[0].binary, true);
});

Deno.test("parseDiff: multiple files returns all", () => {
  const result = parseDiff(multipleFilesDiff);
  assertEquals(result.length, 3);
  assertEquals(result[0].path, "file1.txt");
  assertEquals(result[0].status, "added");
  assertEquals(result[1].path, "file2.txt");
  assertEquals(result[1].status, "modified");
  assertEquals(result[2].path, "file3.txt");
  assertEquals(result[2].status, "deleted");
});

Deno.test("parseDiff: handles no newline at EOF", () => {
  const result = parseDiff(noNewlineDiff);
  assertEquals(result.length, 1);
  assertEquals(result[0].path, "noeof.txt");
});

Deno.test("parseDiff: hunk parsing captures correct line numbers", () => {
  const result = parseDiff(modifiedFileDiff);
  const hunk = result[0].hunks[0];
  assertEquals(hunk.oldStart, 1);
  assertEquals(hunk.oldLines, 3);
  assertEquals(hunk.newStart, 1);
  assertEquals(hunk.newLines, 4);
});

Deno.test("parseDiff: unmerged file handled gracefully", () => {
  const result = parseDiff(unmergedFileDiff);
  assertEquals(result.length, 1);
  assertEquals(result[0].path, "conflicted.txt");
});
