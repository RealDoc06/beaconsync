import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { App } from './App'
import './index.css'

const client = new QueryClient({
  defaultOptions: {
    // Local disk state changes outside the app (Prism, file manager), so refetch on focus,
    // but only when stale to avoid refetch storms.
    queries: { staleTime: 30_000, retry: 1, refetchOnWindowFocus: true },
  },
})

// React Query's focus refetch only follows `visibilitychange`, which a desktop window behind
// other windows never fires. Window focus runs the same refetch of stale queries, so coming
// back to the app shows what changed on disk meanwhile (e.g. an update pulled in game).
window.addEventListener('focus', () => client.getQueryCache().onFocus())

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <QueryClientProvider client={client}>
      <App />
    </QueryClientProvider>
  </StrictMode>,
)
