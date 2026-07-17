import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import type { ReactNode } from 'react'

const HEADING_2 = 'mt-6 mb-2 text-ui-title font-semibold text-foreground-light dark:text-foreground-dark'
const HEADING_3 = 'mt-5 mb-1.5 text-ui-section font-semibold text-foreground-light dark:text-foreground-dark'
const CHIP =
  'rounded bg-black/[0.05] px-1.5 py-0.5 font-mono text-ui-caption text-secondary-light dark:bg-white/[0.08] dark:text-secondary-dark'

interface MarkdownDocProps {
  text: string
  className?: string
}

/**
 * Reading-grade markdown for task briefs and agent output.
 * Security: default escaping only — never add rehype-raw here.
 */
export function MarkdownDoc({ text, className }: MarkdownDocProps) {
  return (
    <div
      className={[
        'text-ui-body leading-relaxed text-foreground-light dark:text-foreground-dark',
        className ?? '',
      ].join(' ')}
    >
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          // page owns the single real <h1>; demote content headings one level
          h1: ({ children }) => <h2 className={HEADING_2}>{children}</h2>,
          h2: ({ children }) => <h3 className={HEADING_3}>{children}</h3>,
          h3: ({ children }) => <h4 className={HEADING_3}>{children}</h4>,
          p: ({ children }) => <p className="my-2">{children}</p>,
          ul: ({ children }) => <ul className="my-2 list-disc space-y-1 pl-5">{children}</ul>,
          ol: ({ children }) => <ol className="my-2 list-decimal space-y-1 pl-5">{children}</ol>,
          li: ({ children }) => <li>{children}</li>,
          a: ({ href, children }) => (
            <a
              href={href}
              target="_blank"
              rel="noopener noreferrer"
              className="text-apple-blue underline-offset-2 hover:underline"
            >
              {children}
            </a>
          ),
          code: (props) => <InlineOrBlockCode {...props} />,
          pre: ({ children }) => (
            <pre className="my-3 overflow-x-auto rounded-card border border-black/[0.08] bg-black/[0.02] p-3 font-mono text-ui-caption leading-relaxed dark:border-white/[0.1] dark:bg-white/[0.04]">
              {children}
            </pre>
          ),
          blockquote: ({ children }) => (
            <blockquote className="my-3 border-l-2 border-black/[0.12] pl-3 text-secondary-light dark:border-white/[0.16] dark:text-secondary-dark">
              {children}
            </blockquote>
          ),
          hr: () => <hr className="my-4 border-black/[0.06] dark:border-white/[0.08]" />,
          table: ({ children }) => (
            <div className="my-3 overflow-x-auto">
              <table className="w-full text-left text-ui-body">{children}</table>
            </div>
          ),
          th: ({ children }) => (
            <th className="border-b border-black/[0.08] px-2 py-1.5 text-ui-caption font-medium text-secondary-light dark:border-white/[0.1] dark:text-secondary-dark">
              {children}
            </th>
          ),
          td: ({ children }) => (
            <td className="border-b border-black/[0.06] px-2 py-1.5 dark:border-white/[0.08]">
              {children}
            </td>
          ),
        }}
      >
        {text}
      </ReactMarkdown>
    </div>
  )
}

function InlineOrBlockCode({
  className,
  children,
}: {
  className?: string
  children?: ReactNode
}) {
  // react-markdown passes language-* className for fenced blocks; those render
  // inside our <pre> mapping, so only style the INLINE case here.
  const isBlock = typeof className === 'string' && className.startsWith('language-')
  if (isBlock) return <code className={className}>{children}</code>
  return <code className={CHIP}>{children}</code>
}
