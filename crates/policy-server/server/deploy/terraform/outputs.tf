output "cloudsql_private_ip" {
  description = "Cloud SQL instance private IP."
  value       = google_sql_database_instance.pasu.private_ip_address
}

output "redis_host" {
  description = "Memorystore host IP."
  value       = google_redis_instance.pasu.host
}

output "redis_port" {
  description = "Memorystore port."
  value       = google_redis_instance.pasu.port
}

output "artifact_registry_repo" {
  description = "Fully-qualified Artifact Registry Docker repo path."
  value       = "${var.region}-docker.pkg.dev/${var.project_id}/${google_artifact_registry_repository.pasu.repository_id}"
}

output "gke_cluster_name" {
  description = "GKE cluster name for get-credentials."
  value       = google_container_cluster.autopilot.name
}

# Ready-to-use connection strings for the kubectl secret (Task 12).
output "database_url" {
  description = "DATABASE_URL for the k8s secret."
  value       = "postgres://pasu:${random_password.db.result}@${google_sql_database_instance.pasu.private_ip_address}:5432/pasu?sslmode=require"
  sensitive   = true
}

output "redis_url" {
  description = "REDIS_URL for the k8s secret."
  value       = "redis://${google_redis_instance.pasu.host}:6379"
}
