# This script takes care of testing your crate

set -ex

# TODO This is the "test phase", tweak it as you see fit
main() {
    # cross build --target $TARGET
    # cross build --target $TARGET --release
    cd rlay-client && cargo build -p rlay-client --features backend_neo4j && cd ..
    cargo build -p rlay-backend
    cargo build -p rlay-backend-neo4j
    cargo build -p rlay-resolve

    if [ ! -z $DISABLE_TESTS ]; then
        return
    fi

    # cross test --target $TARGET -- --test-threads=1
    # cross test --target $TARGET --release
    cd rlay-client && cargo test -- --test-threads=1 --nocapture

    # cross run --target $TARGET
    # cross run --target $TARGET --release
}

# we don't run the "test phase" when doing deploys
if [ -z $TRAVIS_TAG ]; then
    main
fi
