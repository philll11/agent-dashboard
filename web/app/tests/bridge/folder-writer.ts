// Writes synthetic Claude Code and Copilot CLI folders on a thread of their own, for `prepare`, so
// that the process that asked for them, such as the test bridge's server, stays free to answer
// meanwhile.
//
// Node loads this file as it is, so it keeps to TypeScript that only needs its types removed.

import { dirname, join } from "node:path";
import { parentPort, workerData } from "node:worker_threads";

import { type Options, writeClaudeFolder, writeCopilotFolder } from "./synthetic.ts";

const { root, options } = workerData as { root: string; options: Options | undefined };
const synthetic = writeClaudeFolder(root, options);
// Copilot CLI's folder sits beside Claude Code's, where `prepare` points COPILOT_HOME.
if (!options?.noCopilot) writeCopilotFolder(join(dirname(root), ".copilot"), options);
parentPort?.postMessage(synthetic);
