#!/usr/bin/env -S deno run --allow-run=git

import { runGitCommand } from "./main.ts"
import { Effect } from "effect"

const main = () => {
  Effect.runPromise(runGitCommand(["diff"]))
    .then((diff) => {
      console.log("Git Diff Output:");
      console.log(diff);
    })
    .catch((error) => {
      console.error("Error:", error.message);
    });
};

main();
