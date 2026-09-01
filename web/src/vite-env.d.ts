/// <reference types="vite/client" />

// ResultRecord JSONL files imported verbatim from ../results.
declare module '*.jsonl?raw' {
  const content: string;
  export default content;
}
