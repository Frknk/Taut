# infra/terraform (semana 5)

Aquí vive la infraestructura como código. Todo lo que corre en la nube nace de `terraform apply`;
nada se crea a mano en la consola del proveedor. El proveedor (OCI, Azure o AWS) se decide en la
semana 2 según créditos disponibles.

Módulos previstos, con backend remoto para el estado (bucket del proveedor + lock):

1. `network`: VPC / VCN, subredes, security groups.
2. `cluster`: Kubernetes gestionado con 2 nodos y autoescalado a 4.
3. `data`: PostgreSQL gestionado con TimescaleDB, Redis.
4. `registry`: registro de contenedores privado.
5. `addons` (proveedor helm): ingress-nginx, cert-manager, NATS, kube-prometheus-stack, ELK.

En CI, un job `plan` comentará el `terraform plan` en cada PR.
