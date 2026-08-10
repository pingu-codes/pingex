import { rmSync } from "node:fs";

/** Clear previous captures so a renamed or removed shot never lingers in the
 * output directory and end up in a screenshot set by accident. */
export default function globalSetup(): void {
  rmSync("demo/screenshots", { recursive: true, force: true });
}
