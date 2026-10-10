import { useRef, useState, type ReactNode } from "react";

/** Resizable split: canvas left, table right. Both read the same flow store. */
export function HybridView({ left, right }: { left: ReactNode; right: ReactNode }) {
  const [ratio, setRatio] = useState(() => {
    try {
      return Number(localStorage.getItem("nexus.hybridRatio")) || 0.58;
    } catch {
      return 0.58;
    }
  });
  const box = useRef<HTMLDivElement>(null);

  const onDown = (e: React.PointerEvent) => {
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => {
      const r = box.current!.getBoundingClientRect();
      setRatio(Math.min(0.85, Math.max(0.2, (ev.clientX - r.left) / r.width)));
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      try {
        localStorage.setItem("nexus.hybridRatio", String(ratioRef.current));
      } catch {
        /* ignore */
      }
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };
  const ratioRef = useRef(ratio);
  ratioRef.current = ratio;

  return (
    <div className="hybrid" ref={box}>
      <div className="hybrid-pane" style={{ width: `${ratio * 100}%` }}>
        {left}
      </div>
      <div className="hybrid-divider" onPointerDown={onDown} />
      <div className="hybrid-pane" style={{ flex: 1 }}>
        {right}
      </div>
    </div>
  );
}
