You can download the latest pre-built `trail` binary for Mac, Windows and Linux
from [GitHub](https://github.com/trailbaseio/trailbase/releases/).

Alternatively, you can use a Docker image from DockerHub:

```bash
$ alias trail="docker run \
      -p 4000:4000 \
      --mount type=bind,source=$PWD/traildepot,target=/app/traildepot \
      trailbase/trailbase /app/trail"
$ mkdir traildepot # pre-create docker bind-mount path
```

or compile from [source](https://github.com/trailbaseio/trailbase).
