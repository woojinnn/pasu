import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.tsx'

// NOTE: <StrictMode> is intentionally omitted. In dev it double-invokes effects,
// which causes the SSE useEffect in App to open two EventSource connections at
// once — each captured RPC event then arrives at the panel twice. Production
// builds wouldn't have this issue, but the live-event panel is dev-only, so we
// just drop StrictMode here.
createRoot(document.getElementById('root')!).render(<App />)
