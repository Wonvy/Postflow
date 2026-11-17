import "@fontsource-variable/inter";
import "./styles.css";

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { TooltipProvider } from "./components/ui";
import { captureErrors } from "./lib/errors";

captureErrors();

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // 数据就在本机，变化由核心的事件通知，不需要靠轮询或窗口聚焦来刷新
      staleTime: Number.POSITIVE_INFINITY,
      refetchOnWindowFocus: false,
      retry: 1,
    },
  },
});

createRoot(document.getElementById("root") as HTMLElement).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <TooltipProvider delayDuration={450} skipDelayDuration={150}>
        <App />
      </TooltipProvider>
    </QueryClientProvider>
  </StrictMode>,
);
