import { startGoogleLogin } from "@scopeball/api-client";

/**
 * Phase 0 minimal login page. Server-driven OAuth: clicking the button
 * does a full-page nav to `${SERVER_BASE_URL}/auth/google` which then
 * redirects back to `/auth/callback#access_token=…`.
 */
export function LoginPage() {
  return (
    <div style={pageStyle}>
      <div style={cardStyle}>
        <h1 style={{ margin: "0 0 8px 0" }}>🛡 Scopeball</h1>
        <p style={{ margin: "0 0 20px 0", opacity: 0.7, fontSize: 14 }}>
          Wallet transaction safety simulator + Cedar policy enforcement.
        </p>
        <button onClick={startGoogleLogin} style={btnStyle}>
          Sign in with Google
        </button>
      </div>
    </div>
  );
}

const pageStyle: React.CSSProperties = {
  minHeight: "100vh",
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  background: "#f7f7f8",
};
const cardStyle: React.CSSProperties = {
  background: "white",
  padding: 32,
  borderRadius: 8,
  border: "1px solid #e2e2e6",
  maxWidth: 360,
  width: "100%",
  textAlign: "center",
};
const btnStyle: React.CSSProperties = {
  fontSize: 14,
  padding: "10px 20px",
  borderRadius: 6,
  border: "1px solid #0066cc",
  background: "#0066cc",
  color: "white",
  cursor: "pointer",
  width: "100%",
};
