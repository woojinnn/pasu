resource "google_artifact_registry_repository" "pasu" {
  location      = var.region
  repository_id = "pasu"
  format        = "DOCKER"
  description   = "pasu policy-server container images"
}
