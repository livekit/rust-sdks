#!/bin/bash

git apply -p1 libwebrtc-patches/h264_encoder_impl_cc.patch
if [ $? -eq 0 ]; then
  echo "h264_encoder_impl_cc.patch applied successfully."
else
  echo "Failed to apply h264_encoder_impl_cc.patch."
  exit 1
fi

git apply -p1 libwebrtc-patches/h264_encoder_impl_h.patch
if [ $? -eq 0 ]; then
  echo "h264_encoder_impl_h.patch applied successfully."
else
  echo "Failed to apply h264_encoder_impl_h.patch."
  exit 1
fi

git apply -p1 libwebrtc-patches/build_gn.patch
if [ $? -eq 0 ]; then
  echo "build_gn.patch applied successfully."
else
  echo "Failed to apply build_gn.patch."
  exit 1
fi