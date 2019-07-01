#!/usr/bin/env bash

set -eu

td=$(mktemp -d)
pushd $td

git clone git@github.com:timberio/homebrew-brew.git
cd homebrew-brew

package_url="https://packages.timber.io/vector/$VERSION/vector-$VERSION-x86_64-apple-darwin.tar.gz"
package_sha256=$(curl -s $package_url | sha256sum | cut -d " " -f 1)
echo $package_sha256

new_content=$(cat Formula/vector.rb | \
  sed "s|url \".*\"|url \"$package_url\"|" | \
  sed "s|sha256 \".*\"|sha256 \"$package_sha256\"|")

echo "$new_content" > Formula/vector.rb

scripts/test

git commit -am "Release Vector $VERSION"
git push

popd
rm -rf $td