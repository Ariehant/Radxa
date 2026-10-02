import { useEffect, useRef, useState, type ReactNode } from "react";

interface Props<T> {
  items: T[];
  rowHeight: number;
  renderRow: (item: T, index: number) => ReactNode;
  overscan?: number;
  className?: string;
  getKey?: (item: T, index: number) => string | number;
  /** Called with the currently visible index range (for viewport-priority work). */
  onRangeChange?: (start: number, end: number) => void;
}

/** Fixed-row-height windowed list. Only rows in (and near) the viewport mount. */
export function VirtualList<T>({ items, rowHeight, renderRow, overscan = 8, className, getKey, onRangeChange }: Props<T>) {
  const ref = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [height, setHeight] = useState(600);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setHeight(el.clientHeight));
    ro.observe(el);
    setHeight(el.clientHeight);
    return () => ro.disconnect();
  }, []);

  const start = Math.max(0, Math.floor(scrollTop / rowHeight) - overscan);
  const end = Math.min(items.length, Math.ceil((scrollTop + height) / rowHeight) + overscan);
  const visStart = Math.floor(scrollTop / rowHeight);
  const visEnd = Math.min(items.length, Math.ceil((scrollTop + height) / rowHeight));
  useEffect(() => {
    onRangeChange?.(visStart, visEnd);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [visStart, visEnd, items]);

  const rows: ReactNode[] = [];
  for (let i = start; i < end; i++) {
    rows.push(
      <div
        key={getKey ? getKey(items[i], i) : i}
        style={{ position: "absolute", top: i * rowHeight, left: 0, right: 0, height: rowHeight }}
      >
        {renderRow(items[i], i)}
      </div>,
    );
  }

  return (
    <div
      ref={ref}
      className={className}
      style={{ overflowY: "auto", position: "relative", height: "100%" }}
      onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
    >
      <div style={{ height: items.length * rowHeight, position: "relative" }}>{rows}</div>
    </div>
  );
}
