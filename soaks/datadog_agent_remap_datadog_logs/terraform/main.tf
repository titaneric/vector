terraform {
  required_providers {
    kubernetes = {
      version = "~> 2.5.0"
      source  = "hashicorp/kubernetes"
    }
  }
}

provider "kubernetes" {
  config_path = "~/.kube/config"
}


resource "kubernetes_namespace" "soak" {
  metadata {
    name = "soak"
  }
}

module "vector" {
  source       = "../../common/terraform/modules/vector"
  type         = var.type
  vector_image = var.vector_image
  sha          = var.sha
  test_name    = "datadog_agent_remap_datadog_logs"
  vector-toml  = file("${path.module}/vector.toml")
  namespace    = kubernetes_namespace.soak.metadata[0].name
  depends_on   = [module.http-blackhole]
}
module "http-blackhole" {
  source              = "../../common/terraform/modules/lading_http_blackhole"
  type                = var.type
  http-blackhole-toml = file("${path.module}/http_blackhole.toml")
  namespace           = kubernetes_namespace.soak.metadata[0].name
}
module "http-gen" {
  source        = "../../common/terraform/modules/lading_http_gen"
  type          = var.type
  http-gen-toml = file("${path.module}/http_gen.toml")
  namespace     = kubernetes_namespace.soak.metadata[0].name
}
