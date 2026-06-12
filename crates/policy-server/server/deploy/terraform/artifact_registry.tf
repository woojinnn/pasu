resource "google_artifact_registry_repository" "dambi" {
  location      = var.region
  repository_id = "dambi"
  format        = "DOCKER"
  description   = "dambi policy-server container images"
}
