resource "google_container_cluster" "autopilot" {
  name             = "dambi-autopilot"
  location         = var.region
  enable_autopilot = true

  network    = google_compute_network.vpc.id
  subnetwork = google_compute_subnetwork.subnet.id

  ip_allocation_policy {
    cluster_secondary_range_name  = "pods"
    services_secondary_range_name = "services"
  }

  # Allow `terraform destroy` to remove the cluster (default is true in v6).
  deletion_protection = false
}
