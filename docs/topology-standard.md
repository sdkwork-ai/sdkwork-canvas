# Canvas Topology Standard

Canonical topology authority for this application:

- Machine contract: `specs/topology.spec.json` (schemaVersion 5)
- Profile env files: `etc/topology/{deploymentProfile}.{environment}.env`
- Deploy manifest: `deployments/deploy.yaml` (version 2)
- Installed runtime paths: `etc/README.md` (`APPLICATION_DEPLOY_LAYOUT_SPEC.md`)

Profile ids are `<deploymentProfile>.<environment>` only. Retired segments such as
`split-services` and `unified-process` must not appear in active installers,
scripts, or docs.
