resource "google_compute_network" "vpc" {
  name                    = "dambi-vpc"
  auto_create_subnetworks = false
}

resource "google_compute_subnetwork" "subnet" {
  name                     = "dambi-subnet"
  region                   = var.region
  network                  = google_compute_network.vpc.id
  ip_cidr_range            = "10.10.0.0/20"
  private_ip_google_access = true

  secondary_ip_range {
    range_name    = "pods"
    ip_cidr_range = "10.32.0.0/16"
  }

  secondary_ip_range {
    range_name    = "services"
    ip_cidr_range = "10.33.0.0/20"
  }
}

# Reserved internal range Google carves Cloud SQL + Memorystore private IPs from.
# A /16 is generous so both services fit without CIDR pressure.
resource "google_compute_global_address" "psa_range" {
  name          = "dambi-psa-range"
  purpose       = "VPC_PEERING"
  address_type  = "INTERNAL"
  prefix_length = 16
  network       = google_compute_network.vpc.id
}

# VPC peering to Google's service producer network (private services access).
# deletion_policy=ABANDON lets `terraform destroy` complete cleanly (the peering
# is otherwise sticky and can block teardown).
resource "google_service_networking_connection" "psa" {
  network                 = google_compute_network.vpc.id
  service                 = "servicenetworking.googleapis.com"
  reserved_peering_ranges = [google_compute_global_address.psa_range.name]
  deletion_policy         = "ABANDON"
}
