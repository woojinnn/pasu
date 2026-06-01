//! `/docs` + `/openapi.yaml` — public, no-auth API browser.
//! Loads the `OpenAPI` spec from `openapi.yaml` (embedded at compile
//! time via `include_str!`) and renders a Swagger UI page that pulls
//! the JS/CSS from the public unpkg CDN. Keeps the server zero-asset —
//! nothing to copy at deploy time, nothing to chmod.

use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

const OPENAPI_YAML: &str = include_str!("../openapi.yaml");

const SWAGGER_HTML: &str = r##"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Scopeball API</title>
  <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css" />
  <style>
    body { margin: 0; background: #fafafa; }
    .topbar { display: none; }
  </style>
</head>
<body>
  <div id="swagger-ui"></div>
  <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
  <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-standalone-preset.js"></script>
  <script>
    // Pick up a JWT passed as a URL fragment (#token=...). Hash beats
    // query because tokens in query strings end up in access logs.
    function readTokenFromHash() {
      const h = window.location.hash || "";
      const m = h.match(/(?:^|[#&])token=([^&]+)/);
      return m ? decodeURIComponent(m[1]) : null;
    }
    // Falls back to localStorage (same-origin only — won't see the
    // dashboard's token, but lets a user paste once and have it stick).
    function readTokenFromStorage() {
      try { return window.localStorage.getItem("scopeball_docs_jwt"); }
      catch { return null; }
    }
    function persistToken(t) {
      try { window.localStorage.setItem("scopeball_docs_jwt", t); }
      catch { /* private mode */ }
    }
    window.onload = () => {
      const token = readTokenFromHash() || readTokenFromStorage();
      window.ui = SwaggerUIBundle({
        url: "/openapi.yaml",
        dom_id: "#swagger-ui",
        deepLinking: true,
        persistAuthorization: true,
        presets: [
          SwaggerUIBundle.presets.apis,
          SwaggerUIStandalonePreset,
        ],
        layout: "BaseLayout",
        onComplete: () => {
          if (token) {
            window.ui.preauthorizeApiKey("bearerAuth", token);
            persistToken(token);
            // Clear the hash so the token doesn't sit in the URL bar /
            // browser history.
            if (window.location.hash.includes("token=")) {
              history.replaceState(null, "", window.location.pathname);
            }
          }
        },
      });
    };
  </script>
</body>
</html>"##;

/// `GET /docs` — Swagger UI HTML page.
pub async fn docs_html() -> Response {
    (
        StatusCode::OK,
        [
            (CONTENT_TYPE, "text/html; charset=utf-8"),
            (CACHE_CONTROL, "public, max-age=300"),
        ],
        SWAGGER_HTML,
    )
        .into_response()
}

/// `GET /openapi.yaml` — the spec consumed by Swagger UI (and anyone
/// else that wants to codegen a client).
pub async fn openapi_yaml() -> Response {
    (
        StatusCode::OK,
        [
            (CONTENT_TYPE, "application/yaml; charset=utf-8"),
            (CACHE_CONTROL, "public, max-age=300"),
        ],
        OPENAPI_YAML,
    )
        .into_response()
}
