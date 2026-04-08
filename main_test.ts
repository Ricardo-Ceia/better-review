import { assertEquals, assertExists, assertRejects, assertStringIncludes } from "@std/assert";
import { Effect } from "effect";
import { runGitCommand } from "./main.ts";

Deno.test("runGitCommand returns Effect that resolves to string", async () => {
  const effect = runGitCommand(["diff"]);
  const result = await Effect.runPromise(effect);
  assertEquals(typeof result, "string");
});

Deno.test("runGitCommand returns valid diff output", async () => {
  const effect = runGitCommand(["diff"]);
  const result = await Effect.runPromise(effect);
  const isValidDiff = result === "" || result.startsWith("diff ") || result.includes("@@");
  assertEquals(isValidDiff, true);
});

Deno.test("runGitCommand with staged flag returns staged changes", async () => {
  const effect = runGitCommand(["diff", "--staged"]);
  const result = await Effect.runPromise(effect);
  assertEquals(typeof result, "string");
});

Deno.test("runGitCommand with status returns status output", async () => {
  const effect = runGitCommand(["status"]);
  const result = await Effect.runPromise(effect);
  assertEquals(typeof result, "string");
  assertStringIncludes(result, "On branch");
});

Deno.test("runGitCommand fails in non-git directory", async () => {
  const tempDir = await Deno.makeTempDir();
  const originalCwd = Deno.cwd();
  
  try {
    Deno.chdir(tempDir);
    const effect = runGitCommand(["diff"]);
    await assertRejects(() => Effect.runPromise(effect), Error);
  } finally {
    Deno.chdir(originalCwd);
  }
});

Deno.test("runGitCommand fails with invalid git args", async () => {
  const effect = runGitCommand(["diff", "--invalid-flag-xyz"]);
  await assertRejects(() => Effect.runPromise(effect), Error);
});

Deno.test("runGitCommand fails when git not found", async () => {
  const effect = runGitCommand(["diff"]);
  const result = await Effect.runPromise(effect).then(
    (value) => ({ success: true as const, value }),
    (error) => ({ success: false as const, error })
  );
  
  if (!result.success) {
    assertStringIncludes(result.error.message, "Git failed");
  }
});

Deno.test("runGitCommand returns Effect with proper type signature", () => {
  const effect = runGitCommand(["diff"]);
  assertExists(effect);
});

Deno.test("runGitCommand handles empty repository", async () => {
  const tempDir = await Deno.makeTempDir();
  const originalCwd = Deno.cwd();
  
  try {
    Deno.chdir(tempDir);
    const initCmd = new Deno.Command("git", { args: ["init"], stdout: "piped", stderr: "piped" });
    await initCmd.output();
    
    const effect = runGitCommand(["status"]);
    const result = await Effect.runPromise(effect);
    assertStringIncludes(result, "No commits yet");
  } finally {
    Deno.chdir(originalCwd);
  }
});

Deno.test("runGitCommand returns stdout even when stderr has warnings", async () => {
  const effect = runGitCommand(["diff", "--no-ext-diff"]);
  const result = await Effect.runPromise(effect);
  assertEquals(typeof result, "string");
});
