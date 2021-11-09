resource "kubernetes_config_map" "vector" {
  metadata {
    name      = "vector"
    namespace = var.namespace
  }

  data = {
    "vector.toml" = var.vector-toml
  }
}

resource "kubernetes_service" "vector" {
  metadata {
    name      = "vector"
    namespace = var.namespace
  }
  spec {
    selector = {
      type      = var.type
      soak_test = kubernetes_deployment.vector.metadata.0.labels.soak_test
    }
    session_affinity = "ClientIP"
    port {
      name        = "source"
      port        = 8282
      target_port = 8282
    }
    port {
      name        = "prom-export"
      port        = 9090
      target_port = 9090
    }
    type = "ClusterIP"
  }
}


resource "kubernetes_deployment" "vector" {
  metadata {
    name      = "vector"
    namespace = var.namespace
    labels = {
      type      = var.type
      soak_test = var.test_name
    }
  }

  spec {
    replicas = 1

    selector {
      match_labels = {
        type      = var.type
        soak_test = var.test_name
      }
    }

    template {
      metadata {
        labels = {
          type      = var.type
          soak_test = var.test_name
        }
        annotations = {
          "prometheus.io/scrape" = true
          "prometheus.io/port"   = 9090
          "prometheus.io/path"   = "/metrics"
        }
      }

      spec {
        automount_service_account_token = false
        container {
          image_pull_policy = "IfNotPresent"
          image             = var.vector_image
          name              = "vector"

          volume_mount {
            mount_path = "/var/lib/vector"
            name       = "var-lib-vector"
          }

          volume_mount {
            mount_path = "/etc/vector"
            name       = "etc-vector"
            read_only  = true
          }

          resources {
            limits = {
              cpu    = var.vector_cpus
              memory = "512Mi"
            }
            requests = {
              cpu    = var.vector_cpus
              memory = "512Mi"
            }
          }

          port {
            container_port = 8282
            name           = "source"
          }
          port {
            container_port = 9090
            name           = "prom-export"
          }

          liveness_probe {
            http_get {
              port = 9090
              path = "/metrics"
            }
          }
        }

        volume {
          name = "var-lib-vector"
          empty_dir {}
        }
        volume {
          name = "etc-vector"
          config_map {
            name = kubernetes_config_map.vector.metadata[0].name
          }
        }

      }
    }
  }
}
