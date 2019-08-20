#!/usr/bin/env bash

# version.sh
#
# SUMMARY
#
#   Responsible for computing the official relase version of Vector.
#   This is based off of the latest git tag. If we're operting on a tag
#   then the raw tag name will be used, otherwise a tag / commit combination
#   will be used. Ex: 0.2.0-93-gcc92bf6

set -e

on_git_tag=$(git describe --exact-match --tags HEAD 2> /dev/null || echo "")
latest_version=$1

if [ -z "${on_git_tag}" ] && [ "$latest_version" != "true" ]
then
  git describe --tags | sed 's/^v//g'
else
  git describe --abbrev=0 --tags | sed 's/^v//g'
fi