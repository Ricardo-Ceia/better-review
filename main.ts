import {Effect} from "effect"

export const runGitCommand = (args: string[]): Effect.Effect<string, Error, never> =>
Effect.tryPromise({
  try: async () => {
    const process = new Deno.Command("git", {
      args,
      stdout: "piped",
      stderr: "piped",
    });
    const output = await process.output();
    const decoder = new TextDecoder();
    const stdout = decoder.decode(output.stdout);
    const stderr = decoder.decode(output.stderr);
    if (output.code !== 0) {
      throw new Error(`${stderr}`);
    }
    return stdout;
  },
  catch: (error) => new Error(`Git failed: ${error}`),
});
