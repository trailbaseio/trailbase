You can download the latest pre-built `trail` binary for Mac and Linux from
[GitHub](https://github.com/trailbaseio/trailbase/releases/).

Alternatively, you can run TrailBase straight from DockerHub:

```bash
$ alias trail=docker run \
      -p 4000:4000 \
      --mount type=bind,source=$PWD/traildepot,target=/app/traildepot \
      trailbase/trailbase /app/trail
```

or compile it yourself from [source](https://github.com/trailbaseio/trailbase).
