#ifndef VAAPI_ENCODER_WRAPPER_H_
#define VAAPI_ENCODER_WRAPPER_H_

#include <va/va.h>
#include <va/va_enc_h264.h>

#include <stdbool.h>
#include <vector>

namespace livekit {

    #define SURFACE_NUM 16 /* 16 surfaces for reference */

    typedef struct {
        VAProfile       h264_profile;
        int             h264_entropy_mode;
        int             frame_width;
        int             frame_height;
        int             frame_rate;
        unsigned int    frame_bitrate;
        int             initial_qp;
        int             minimal_qp;
        int             intra_period;
        int             intra_idr_period;
        int             ip_period;
        int             rc_mode;
    } VA264Config;

class VaapiEncoderWrapper {

public:
    VaapiEncoderWrapper();
    ~VaapiEncoderWrapper();

    // Initialize the encoder with the given parameters.
    bool Initialize(int width, int height, int bitrate, int intra_period, int idr_period, int ip_period, int frame_rate, int profile, int rc_mode);

    // Encode a frame and return the encoded data.
    bool  Encode(void * ctx, int fourcc, uint8_t * y, uint8_t * u, uint8_t * v, int * encodedsize, bool forceIDR, std::vector<uint8_t>& encoded_data);

    // Release resources.
    void Release();

private:
    VADisplay                           va_dpy;

    VAConfigAttrib                      attrib[VAConfigAttribTypeMax];
    VAConfigAttrib                      config_attrib[VAConfigAttribTypeMax];
    int                                 config_attrib_num;
    int                                 enc_packed_header_idx;
    VASurfaceID                         src_surface[SURFACE_NUM];
    VABufferID                          coded_buf[SURFACE_NUM];
    VASurfaceID                         ref_surface[SURFACE_NUM];
    VAConfigID                          config_id;
    VAContextID                         context_id;
    VAEncSequenceParameterBufferH264    seq_param;
    VAEncPictureParameterBufferH264     pic_param;
    VAEncSliceParameterBufferH264       slice_param;
    VAPictureH264                       CurrentCurrPic;
    VAPictureH264                       ReferenceFrames[SURFACE_NUM];
    VAPictureH264                       RefPicList0_P[SURFACE_NUM * 2];
    VAPictureH264                       RefPicList0_B[SURFACE_NUM * 2];
    VAPictureH264                       RefPicList1_B[SURFACE_NUM * 2];

    // Default entrypoint for Encode
    VAEntrypoint                        requested_entrypoint;
    VAEntrypoint                        selected_entrypoint;

    unsigned int                        numShortTerm;
    int                                 constraint_set_flag;
    int                                 h264_packedheader; /* support pack header? */
    int                                 h264_maxref;
    int                                 frame_width_mbaligned;
    int                                 frame_height_mbaligned;
    unsigned int                        current_frame_num;
    int                                 current_frame_type;
    unsigned long long                  current_frame_encoding;
    unsigned long long                  current_frame_display;
    unsigned long long                  current_IDR_display;

    uint8_t *                           encoded_buffer;
    VA264Config config;
} // namespace livekit

#endif // VAAPI_ENCODER_WRAPPER_H_
