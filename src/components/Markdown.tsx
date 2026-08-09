import { memo } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeHighlight from "rehype-highlight";

/** Markdown for COMPLETED assistant text. Streaming text stays plain — the
 *  parse only happens once per item, after ItemCompleted. */
export const Markdown = memo(function Markdown({ text }: { text: string }) {
  return (
    <div className="md">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeHighlight]}
        components={{
          a: (props) => (
            <a {...props} target="_blank" rel="noreferrer" />
          ),
        }}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
});
