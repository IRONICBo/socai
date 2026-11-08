import { createSocaiTools } from "./tools.mjs";

export default function socaiPiExtension(pi) {
  for (const tool of createSocaiTools()) {
    pi.registerTool(tool);
  }
}
