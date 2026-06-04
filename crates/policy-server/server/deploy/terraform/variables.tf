variable "project_id" {
  type        = string
  description = "GCP project ID."
}

variable "region" {
  type        = string
  description = "GCP region for all regional resources."
  default     = "asia-northeast3"
}

variable "db_tier" {
  type        = string
  description = "Cloud SQL machine tier."
  default     = "db-custom-1-3840"
}

variable "db_max_connections" {
  type        = string
  description = "Postgres max_connections flag (string per Cloud SQL API)."
  default     = "100"
}

variable "redis_memory_gb" {
  type        = number
  description = "Memorystore capacity in GiB."
  default     = 1
}
