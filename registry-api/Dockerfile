# Cloud Run image for registry-api. Self-contained — build context is the
# registry-api/ directory only:
#   gcloud builds submit registry-api --tag <REGION>-docker.pkg.dev/.../registry-api:v1
#
# Supply-chain hardening (threat model F4) — PARTIAL by default:
#  - ENFORCED: `npm ci --ignore-scripts` blocks malicious dependency lifecycle
#    scripts (the prod tree has none today), and package-lock.json pins versions.
#  - NOT enforced by default: the base image. The ARG default below is the MUTABLE
#    tag node:20-alpine, so a plain build is NOT digest-pinned. To pin, resolve once
#      docker buildx imagetools inspect node:20-alpine --format '{{.Manifest.Digest}}'
#    and build with --build-arg NODE_IMAGE=node:20-alpine@sha256:<digest> (bake that
#    into deploy/CI). Treat F4's base-image leg as PINNABLE, not closed.
ARG NODE_IMAGE=node:20-alpine
FROM ${NODE_IMAGE} AS build
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --ignore-scripts
COPY tsconfig.json ./
COPY src ./src
RUN npm run build

FROM ${NODE_IMAGE}
ENV NODE_ENV=production
ENV HOST=0.0.0.0
ENV PORT=8080
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --omit=dev --ignore-scripts && chown -R node:node /app
COPY --chown=node:node --from=build /app/dist ./dist
USER node
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD node -e "fetch('http://127.0.0.1:'+(process.env.PORT??'8080')+'/health').then(r=>process.exit(r.ok?0:1)).catch(()=>process.exit(1))"
CMD ["node", "--enable-source-maps", "dist/index.js"]
