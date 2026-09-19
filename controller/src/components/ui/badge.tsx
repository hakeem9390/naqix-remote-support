import type { ComponentProps } from 'react'
import { cn } from '@/lib/utils'

export function Badge({ className, ...props }: ComponentProps<'span'>) {
  return (
    <span
      className={cn(
        'inline-flex items-center rounded-full px-2 py-0.5 text-[11px] font-medium',
        'bg-[hsl(var(--secondary))] text-[hsl(var(--muted-foreground))]',
        className,
      )}
      {...props}
    />
  )
}
