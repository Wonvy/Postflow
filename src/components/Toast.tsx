// 屏幕下方一闪而过的一句话，比如「已发送」。不需要用户做什么，几秒后自己消失。

import { Check } from "lucide-react";
import { useEffect } from "react";
import { create } from "zustand";

interface ToastState {
  message: string | null;
  /** 同一句话连着出现两次时也要重新计时。 */
  seq: number;
  show: (message: string) => void;
  hide: () => void;
}

const useToast = create<ToastState>((set) => ({
  message: null,
  seq: 0,
  show: (message) => set((s) => ({ message, seq: s.seq + 1 })),
  hide: () => set({ message: null }),
}));

export function toast(message: string) {
  useToast.getState().show(message);
}

export function Toaster() {
  const message = useToast((s) => s.message);
  const seq = useToast((s) => s.seq);
  const hide = useToast((s) => s.hide);
  useEffect(() => {
    if (!message) return;
    const timer = setTimeout(hide, 3000);
    return () => clearTimeout(timer);
  }, [message, seq, hide]);
  if (!message) return null;
  return (
    <div
      key={seq}
      role="status"
      className="anim-pop fixed bottom-6 left-1/2 z-50 flex -translate-x-1/2 items-center gap-2 rounded-lg bg-surface px-3.5 py-2 text-[13px] text-fg shadow-pop"
    >
      <Check size={14} strokeWidth={2.25} className="text-accent" />
      {message}
    </div>
  );
}
