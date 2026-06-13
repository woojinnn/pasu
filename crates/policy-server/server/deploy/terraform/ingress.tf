# Global external static IP for the GCE L7 Ingress (M3). Reserved (not ephemeral)
# so the HTTPS endpoint address is stable across redeploys.
resource "google_compute_global_address" "ingress" {
  name = "dambi-ingress-ip"
}
