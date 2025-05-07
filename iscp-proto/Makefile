DIR_DIST := gen

VERSION_PROTOC_GOGOPROTO=v1.3.2
VERSION_PROTOC_GO=v1.26

.PHONY: build-all
build-all: build-proto build-go-gogofast

.PHONY: build-proto
build-proto:
	buf generate proto

.PHONY: build-go-gogofast
build-go-gogofast:
	buf generate proto --template ./buf.gen.gogoproto.yaml

.PHONY: lint
lint:
	buf lint proto

.PHONY: format
format:
	buf format --exit-code -w proto

.PHONY: tools
tools:
	go install github.com/gogo/protobuf/protoc-gen-gogofast@$(VERSION_PROTOC_GOGOPROTO)
	go run github.com/x-motemen/ghq@latest get github.com/gogo/protobuf/gogoproto
	npm install @bufbuild/protobuf @bufbuild/protoc-gen-es @bufbuild/buf

.PHONY: clean
clean:
	rm -rf $(DIR_DIST)
