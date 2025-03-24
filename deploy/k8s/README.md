# Podman/K8s Example Setup

The following command will start TrailBase and bind to `0.0.0.0:4010`:

```bash
$ podman play kube trailbase-deployment.yml --publish=4010:4000
```

To make sure TrailBase has started successfully, you can run:

```bash
$ podman ps
CONTAINER ID  IMAGE                                    COMMAND               CREATED        STATUS        PORTS                     NAMES
2e36f124950e  docker.io/trailbase/trailbase:latest     /app/trail --data...  3 seconds ago  Up 3 seconds  0.0.0.0:4010->4000/tcp    trailbase-deployment-pod-trailbase
```

To get the generated login credentials, check the container logs:

```bash
$ podman logs trailbase-deployment-pod-trailbase
```

Finally, you can browse to
[localhost:4010/_/admin](http://localhost:4010/_/admin) and sign in using the
generated credentials.

Note that the database and other runtime files are persisted in a local volume,
i.e.:

```bash
$ podman volume ls
DRIVER      VOLUME NAME
local       trailbase-storage
```

To use bind-mount a local host path instead, check out the comments in
`trailbase-deployment.yml`.
