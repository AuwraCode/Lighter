import { isValidElement, memo, useState, type ReactNode } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeHighlight from "rehype-highlight";
import { Check, Copy } from "lucide-react";

/** Markdown with syntax highlighting. Completed items parse once; the
 *  streaming tail goes through StreamingMarkdown (throttled re-parse). */
export const Markdown = memo(function Markdown({ text }: { text: string }) {
  return (
    <div className="md">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeHighlight]}
        components={{
          a: (props) => <a {...props} target="_blank" rel="noreferrer" />,
          pre: (props) => <CodeBlock>{props.children}</CodeBlock>,
        }}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
});

/** Fenced code block with a language chip and a copy button. */
function CodeBlock({ children }: { children: ReactNode }) {
  const [copied, setCopied] = useState(false);
  const lang = extractLanguage(children);
  const copy = () => {
    void navigator.clipboard
      .writeText(extractText(children).replace(/\n$/, ""))
      .then(() => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1200);
      })
      .catch(() => {});
  };
  return (
    <div className="group/code relative">
      <pre>{children}</pre>
      <div className="absolute right-1.5 top-1.5 flex items-center gap-1">
        {lang && (
          <span className="rounded bg-raised px-1.5 py-0.5 font-mono text-[10px] text-fg-muted">
            {lang}
          </span>
        )}
        <button
          onClick={copy}
          title="Copy code"
          className="hidden rounded bg-raised p-1 text-fg-muted hover:text-fg group-hover/code:block"
        >
          {copied ? (
            <Check size={11} className="text-success" />
          ) : (
            <Copy size={11} />
          )}
        </button>
      </div>
    </div>
  );
}

function extractLanguage(node: ReactNode): string | null {
  if (isValidElement(node)) {
    const props = node.props as { className?: string; children?: ReactNode };
    const match = /language-([\w+-]+)/.exec(props.className ?? "");
    if (match) return match[1];
    return extractLanguage(props.children ?? null);
  }
  if (Array.isArray(node)) {
    for (const child of node) {
      const lang = extractLanguage(child);
      if (lang) return lang;
    }
  }
  return null;
}

function extractText(node: ReactNode): string {
  if (node == null || typeof node === "boolean") return "";
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(extractText).join("");
  if (isValidElement(node)) {
    return extractText((node.props as { children?: ReactNode }).children ?? null);
  }
  return "";
}
