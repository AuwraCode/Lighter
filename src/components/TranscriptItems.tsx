// Transcript item renderers: memoized so completed items never re-render.

import { memo, useEffect, useMemo, useRef, useState } from "react";
import { AnsiUp } from "ansi_up";
import {
  Brain,
  Check,
  ChevronDown,
  ChevronRight,
  Loader2,
  Squircle,
  TriangleAlert,
} from "lucide-react";
import { cn } from "@/lib/cn";
import type { TranscriptItem } from "@/lib/generated/TranscriptItem";
import type { JsonValue } from "@/lib/generated/serde_json/JsonValue";
import { Markdown } from "./Markdown";

/** Re-parse streamed markdown at most every 150ms — colors and code
 *  highlighting appear WHILE the answer streams, without a parse per batch. */
function useThrottledValue<T>(value: T, ms: number): T {
  const [v, setV] = useState(value);
  const last = useRef(0);
  const timer = useRef<number>(undefined);
  useEffect(() => {
    const elapsed = performance.now() - last.current;
    if (elapsed >= ms) {
      last.current = performance.now();
      setV(value);
    } else {
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => {
        last.current = performance.now();
        setV(value);
      }, ms - elapsed);
    }
    return () => window.clearTimeout(timer.current);
  }, [value, ms]);
  return v;
}

function StreamingMarkdown({ text }: { text: string }) {
  const throttled = useThrottledValue(text, 150);
  return <Markdown text={throttled} />;
}

export const MemoItemView = memo(ItemViewInner);

function ItemViewInner({
  item,
  streaming,
}: {
  item: TranscriptItem;
  streaming: boolean;
}) {
  const nested =
    "parent_tool_use_id" in item && item.parent_tool_use_id
      ? "ml-6 border-l border-border-subtle pl-3"
      : "";
  switch (item.kind) {
    case "UserText":
      return (
        <div
          className={cn(
            "rounded-lg border px-3 py-2 text-sm",
            item.injected
              ? "whitespace-pre-wrap border-warning/30 bg-warning/5 text-warning"
              : "border-accent-muted/60 bg-accent/5",
          )}
        >
          {item.injected ? item.text : <Markdown text={item.text} />}
        </div>
      );
    case "AssistantText":
      return (
        <div className={cn("text-sm leading-relaxed", nested)}>
          {streaming ? (
            <StreamingMarkdown text={item.text} />
          ) : (
            <Markdown text={item.text} />
          )}
        </div>
      );
    case "Thinking":
      return (
        <ThinkingBlock text={item.text} streaming={streaming} className={nested} />
      );
    case "ToolUse":
      return (
        <ToolCard
          name={item.name}
          input={item.input}
          output={item.output}
          className={nested}
        />
      );
    case "CompactMarker":
      return (
        <div className="my-1 flex items-center gap-2 text-[11px] text-fg-muted">
          <div className="h-px flex-1 bg-border" />
          context compacted
          <div className="h-px flex-1 bg-border" />
        </div>
      );
  }
}

function ThinkingBlock({
  text,
  streaming,
  className,
}: {
  text: string;
  streaming: boolean;
  className?: string;
}) {
  const [userExpanded, setUserExpanded] = useState<boolean | null>(null);
  const expanded = userExpanded ?? streaming;
  return (
    <div className={className}>
      <button
        onClick={() => setUserExpanded(!expanded)}
        className="inline-flex items-center gap-1.5 text-[11px] text-fg-muted hover:text-fg-secondary"
      >
        {streaming ? (
          <Loader2 size={11} className="animate-spin" />
        ) : (
          <Brain size={11} />
        )}
        {streaming ? "Thinking…" : "Thought process"}
        {expanded ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
      </button>
      {expanded && (
        <div className="mt-1.5 whitespace-pre-wrap border-l-2 border-border pl-3 text-xs italic leading-relaxed text-fg-muted">
          {text}
        </div>
      )}
    </div>
  );
}

interface ToolOutputShape {
  text: string;
  is_error: boolean;
  truncated: boolean;
}

/** One-line human summary of a tool call. */
function toolSummary(name: string, input: JsonValue): string {
  const obj = (input ?? {}) as Record<string, unknown>;
  switch (name) {
    case "Bash":
    case "PowerShell":
      return String(obj.command ?? obj.description ?? "");
    case "Write":
    case "Edit":
    case "Read":
    case "NotebookEdit":
      return String(obj.file_path ?? "");
    case "Glob":
    case "Grep":
      return String(obj.pattern ?? "");
    case "Task":
      return String(obj.description ?? "");
    case "WebFetch":
    case "WebSearch":
      return String(obj.url ?? obj.query ?? "");
    default: {
      const s = JSON.stringify(input);
      return s === "{}" || s === "null" ? "" : s;
    }
  }
}

function ToolCard({
  name,
  input,
  output,
  className,
}: {
  name: string;
  input: JsonValue;
  output: ToolOutputShape | null;
  className?: string;
}) {
  const failed = output?.is_error ?? false;
  const running = !output;
  const [userExpanded, setUserExpanded] = useState<boolean | null>(null);
  const expanded = userExpanded ?? failed;
  const summary = toolSummary(name, input);

  return (
    <div
      className={cn(
        "overflow-hidden rounded-lg border bg-surface font-mono text-xs",
        failed ? "border-danger/40" : "border-border",
        className,
      )}
    >
      <button
        onClick={() => setUserExpanded(!expanded)}
        className="flex w-full items-center gap-2 px-3 py-1.5 text-left hover:bg-hover"
      >
        <Squircle size={12} className={failed ? "text-danger" : "text-accent"} />
        <span className="shrink-0 font-medium">{name}</span>
        <span className="min-w-0 flex-1 truncate text-fg-muted" title={summary}>
          {summary}
        </span>
        {running ? (
          <Loader2 size={12} className="shrink-0 animate-spin text-fg-muted" />
        ) : failed ? (
          <TriangleAlert size={12} className="shrink-0 text-danger" />
        ) : (
          <Check size={12} className="shrink-0 text-success" />
        )}
        {expanded ? (
          <ChevronDown size={12} className="shrink-0 text-fg-muted" />
        ) : (
          <ChevronRight size={12} className="shrink-0 text-fg-muted" />
        )}
      </button>
      {expanded && (
        <>
          <ToolInput name={name} input={input} />
          {output && (
            <ToolOutputView
              text={output.text}
              truncated={output.truncated}
              failed={failed}
            />
          )}
        </>
      )}
    </div>
  );
}

const ANSI_PATTERN = /\[[0-9;]*m/;

/** Tool output with ANSI colors rendered (test runners, git, linters…). */
function ToolOutputView({
  text,
  truncated,
  failed,
}: {
  text: string;
  truncated: boolean;
  failed: boolean;
}) {
  const html = useMemo(() => {
    if (!ANSI_PATTERN.test(text)) return null;
    const ansi = new AnsiUp();
    ansi.use_classes = false;
    return ansi.ansi_to_html(text); // escapes HTML, converts SGR to spans
  }, [text]);

  const cls = cn(
    "max-h-72 overflow-auto border-t border-border-subtle px-3 py-2",
    failed ? "text-danger" : "text-fg-secondary",
  );
  if (html) {
    return (
      <pre
        className={cls}
        dangerouslySetInnerHTML={{
          __html: html + (truncated ? "\n… (truncated)" : ""),
        }}
      />
    );
  }
  return (
    <pre className={cls}>
      {text || "(no output)"}
      {truncated ? "\n… (truncated)" : ""}
    </pre>
  );
}

function ToolInput({ name, input }: { name: string; input: JsonValue }) {
  const obj = (input ?? {}) as Record<string, unknown>;
  if ((name === "Bash" || name === "PowerShell") && typeof obj.command === "string") {
    return (
      <pre className="max-h-40 overflow-auto border-t border-border-subtle px-3 py-2 text-fg">
        <span className="select-none text-fg-muted">$ </span>
        {obj.command}
      </pre>
    );
  }
  if (name === "Write" && typeof obj.content === "string") {
    return (
      <pre className="max-h-56 overflow-auto border-t border-border-subtle px-3 py-2 text-fg-secondary">
        {obj.content}
      </pre>
    );
  }
  if (name === "Edit" && typeof obj.old_string === "string") {
    return (
      <div className="max-h-56 overflow-auto border-t border-border-subtle px-3 py-2">
        <pre className="whitespace-pre-wrap text-danger/80">
          {prefixLines(String(obj.old_string), "- ")}
        </pre>
        <pre className="mt-1 whitespace-pre-wrap text-success/80">
          {prefixLines(String(obj.new_string ?? ""), "+ ")}
        </pre>
      </div>
    );
  }
  return (
    <pre className="max-h-40 overflow-auto border-t border-border-subtle px-3 py-2 text-fg-secondary">
      {JSON.stringify(input, null, 2)}
    </pre>
  );
}

function prefixLines(text: string, prefix: string): string {
  return text
    .split("\n")
    .map((l) => prefix + l)
    .join("\n");
}
