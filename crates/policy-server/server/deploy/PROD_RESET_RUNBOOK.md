# Production Reset Runbook

Use this only before external launch, or when losing all policy-server data is
acceptable. This keeps the GKE cluster, namespace, Cloud SQL instance, SQL user,
Redis instance, DNS, Ingress, certificate, and Kubernetes secrets. It resets the
Cloud SQL database contents and lets the Helm migration hook rebuild the schema
from an empty database.

## Current Production Targets

- GCP project: `policy-engine-498313`
- GKE cluster: `dambi-autopilot`
- GKE region: `asia-northeast3`
- Kubernetes namespace: `dambi`
- Helm release: `dambi`
- Cloud SQL instance: `dambi-pg`
- Cloud SQL database: `dambi`
- Kubernetes secret: `policy-server-secrets`
- Helm values: `crates/policy-server/server/deploy/helm/policy-server/values-m3.yaml`

## Reset Procedure

1. Point kubectl at production GKE.

   ```sh
   gcloud container clusters get-credentials dambi-autopilot \
     --region asia-northeast3 \
     --project policy-engine-498313
   kubectl config current-context
   ```

2. Snapshot non-secret state.

   ```sh
   mkdir -p /private/tmp/dambi-reset
   helm get values dambi -n dambi > /private/tmp/dambi-reset/helm-values.yaml
   helm get manifest dambi -n dambi > /private/tmp/dambi-reset/helm-manifest.yaml
   kubectl -n dambi get deploy,pod,svc,ingress,managedcertificate,backendconfig -o wide \
     > /private/tmp/dambi-reset/k8s-resources.txt
   ```

3. Stop API and worker pods so PostgreSQL connections close before dropping the
   database.

   ```sh
   kubectl -n dambi scale \
     deploy/dambi-policy-server-api \
     deploy/dambi-policy-server-worker \
     --replicas=0
   kubectl -n dambi wait --for=delete pod \
     -l app.kubernetes.io/instance=dambi \
     --timeout=120s
   ```

4. Drop and recreate the application database. This deletes all application
   data, but keeps the Cloud SQL instance and SQL user.

   ```sh
   gcloud sql databases delete dambi \
     --instance dambi-pg \
     --project policy-engine-498313 \
     --quiet
   gcloud sql databases create dambi \
     --instance dambi-pg \
     --project policy-engine-498313
   ```

5. Reinstall the app with the target image tag. The chart's `policy-server-migrate`
   pre-install/pre-upgrade hook runs `policy-server-migrate`, applies all
   PostgreSQL migrations, then the API and worker roll out.

   ```sh
   IMAGE_TAG="$(git rev-parse --short HEAD)"
   helm upgrade --install dambi \
     crates/policy-server/server/deploy/helm/policy-server \
     -n dambi \
     -f crates/policy-server/server/deploy/helm/policy-server/values-m3.yaml \
     --set image.tag="${IMAGE_TAG}"
   ```

6. Verify rollout and readiness.

   ```sh
   kubectl -n dambi rollout status deploy/dambi-policy-server-api --timeout=180s
   kubectl -n dambi rollout status deploy/dambi-policy-server-worker --timeout=180s
   curl -sS -i https://dambi-policy.duckdns.org/health
   curl -sS -i https://dambi-policy.duckdns.org/readyz
   ```

   `/readyz` should report:

   ```json
   {"status":"ready","checks":{"postgres":"ok","redis":"ok","required_env":"ok","sync_config":"ok"}}
   ```

## Notes

- Do not delete `policy-server-secrets` during this reset; it contains the
  database URL, OAuth settings, JWT secret, and Redis URL.
- Do not delete the Cloud SQL instance unless the GKE/Cloud SQL networking or
  Terraform state is also being intentionally rebuilt.
- `RUN_MIGRATIONS_ON_STARTUP=false` in the Helm chart. Production migrations are
  expected to run through the Helm hook job, not on every API/worker startup.
- If the Helm upgrade fails during the migration hook, keep the deployments at
  zero replicas, inspect the hook pod logs, fix the migration issue, and rerun
  the Helm upgrade.
