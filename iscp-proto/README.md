# iscp-proto

iSCPv2のProtocolBuffer

## Prerequisite

- [buf](https://github.com/bufbuild/buf)
- [npm](https://www.npmjs.com/) ※ Typescriptの生成のみ

   ```bash
   npm install @bufbuild/protobuf @bufbuild/protoc-gen-es @bufbuild/buf
   ```

- [go](https://go.dev/) ※ gogoprotoの生成のみ

   ```bash
    go install github.com/gogo/protobuf/protoc-gen-gogofast@v1.3.2
    go run github.com/x-motemen/ghq@latest get github.com/gogo/protobuf/gogoproto
   ```

## Usage

1. gitサブモジュールとして利用します

    ```bash
    git submodule add https://github.com/aptpod/iscp-proto.git iscp-proto
    # or
    git submodule add git@github.com:aptpod/iscp-proto.git iscp-proto
    ```

2. プロジェクトのルートに`buf.gen.yaml` を作成します。`iscp-proto/buf.gen.yaml` または `iscp-proto/buf.gen.gogoproto.yaml` を参考に編集します。
3. コードを生成します。

```bash
# ソースコードの生成
buf generate iscp-proto/proto
```

`gen` ディレクトリに生成したコードは配置済みです。

## License
