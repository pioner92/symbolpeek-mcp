import React, { createContext, forwardRef, lazy, memo, Suspense, useContext, useMemo, useState } from 'react';

export interface PanelProps {
  title: string;
}

export const PanelContext = createContext<PanelProps | undefined>(undefined);

export function usePanelTitle() {
  const value = useContext(PanelContext);
  return value?.title ?? '';
}

export const Panel = memo(({ title }: PanelProps) => {
  const [open, setOpen] = useState(false);
  const label = useMemo(() => `${title}:${open}`, [title, open]);
  return <button onClick={() => setOpen(!open)}>{label}</button>;
});

export const PanelWithRef = forwardRef<HTMLButtonElement, PanelProps>(function PanelWithRef(props, ref) {
  return <button ref={ref}>{props.title}</button>;
});

export const DeferredPanel = lazy(() => import('./Panel'));

export function PanelProvider({ children }: { children: React.ReactNode }) {
  return <PanelContext.Provider value={{ title: 'default' }}><Suspense fallback={null}>{children}</Suspense></PanelContext.Provider>;
}

