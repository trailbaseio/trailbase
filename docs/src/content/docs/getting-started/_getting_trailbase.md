You can download the latest pre-built `trail` binary for Mac, Windows and Linux
from [GitHub](https://github.com/trailbaseio/trailbase/releases/).

Alternatively, you can use a Docker image from DockerHub:

```bash
$ alias trail="docker run \
      -p 4000:4000 \
      --mount type=bind,source=$PWD/traildepot,target=/app/traildepot \
      trailbase/trailbase /app/trail"
$ mkdir traildepot # pre-create mount point for Docker
$ trail run --address 0.0.0.0:4000
```

or compile from [source](https://github.com/trailbaseio/trailbase).
