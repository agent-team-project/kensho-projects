FROM golang:1.25.6-alpine@sha256:98e6cffc31ccc44c7c15d83df1d69891efee8115a5bb7ede2bf30a38af3e3c92 AS build
WORKDIR /src
COPY go.mod go.sum ./
COPY vendor ./vendor
COPY cmd ./cmd
COPY internal ./internal
RUN --network=none CGO_ENABLED=0 go build -mod=vendor -trimpath -ldflags="-s -w" -o /out/workplane ./cmd/api
RUN --network=none CGO_ENABLED=0 go build -mod=vendor -trimpath -ldflags="-s -w" -o /out/workplane-smoke ./cmd/smoke
RUN --network=none CGO_ENABLED=0 go build -mod=vendor -trimpath -ldflags="-s -w" -o /out/workplane-realtime-smoke ./cmd/realtime-smoke
RUN --network=none CGO_ENABLED=0 go build -mod=vendor -trimpath -ldflags="-s -w" -o /out/workplane-planning-smoke ./cmd/planning-smoke
RUN --network=none CGO_ENABLED=0 go build -mod=vendor -trimpath -ldflags="-s -w" -o /out/workplane-work-smoke ./cmd/work-smoke

FROM scratch AS runtime
COPY --from=build /out/workplane /workplane
COPY web/dist /web
USER 65532:65532
ENV WORKPLANE_WEB_ROOT=/web
EXPOSE 8080
ENTRYPOINT ["/workplane"]

FROM scratch AS smoke
COPY --from=build /out/workplane-smoke /workplane-smoke
COPY --from=build /out/workplane-realtime-smoke /workplane-realtime-smoke
COPY --from=build /out/workplane-planning-smoke /workplane-planning-smoke
COPY --from=build /out/workplane-work-smoke /workplane-work-smoke
USER 65532:65532
ENTRYPOINT ["/workplane-smoke"]
