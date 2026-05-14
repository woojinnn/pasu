# Build from the repository root:
# docker build -f policy-rpc/Dockerfile -t scopeball-policy-rpc .

FROM node:20-alpine AS build

WORKDIR /app

COPY extension/package.json extension/yarn.lock extension/.yarnrc.yml ./extension/
COPY extension/.yarn ./extension/.yarn

WORKDIR /app/extension
RUN yarn install --immutable

WORKDIR /app/policy-rpc
COPY policy-rpc/package.json policy-rpc/tsconfig.json ./
COPY policy-rpc/src ./src
RUN ../extension/node_modules/.bin/tsc -p tsconfig.json

FROM node:20-alpine

ENV NODE_ENV=production
ENV HOST=0.0.0.0
ENV PORT=8787

WORKDIR /app

COPY --chown=node:node policy-rpc/package.json ./
COPY --from=build --chown=node:node /app/policy-rpc/dist ./dist

USER node

EXPOSE 8787

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD node -e "fetch('http://127.0.0.1:' + (process.env.PORT ?? '8787') + '/health').then((response) => process.exit(response.ok ? 0 : 1)).catch(() => process.exit(1))"

CMD ["node", "--enable-source-maps", "dist/index.js"]
