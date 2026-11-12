/* SoX Resampler Library      Copyright (c) 2007-13 robs@users.sourceforge.net
 * Licence for this file: LGPL v2.1                  See LICENCE for details. */

static int * LSX_FFT_BR;
static DFT_FLOAT * LSX_FFT_SC;
static int FFT_LEN;
static ccrw2_t FFT_CACHE_CCRW = CCRW2_INITIALIZER;

void LSX_INIT_FFT_CACHE(void)
{
  /* The empty cache and its lock are initialized statically. */
}

void LSX_CLEAR_FFT_CACHE(void)
{
  ccrw2_become_writer(FFT_CACHE_CCRW);
  free(LSX_FFT_BR);
  free(LSX_FFT_SC);
  LSX_FFT_SC = NULL;
  LSX_FFT_BR = NULL;
  FFT_LEN = 0;
  ccrw2_cease_writing(FFT_CACHE_CCRW);
}

static bool UPDATE_FFT_CACHE(int len)
{
  assert(lsx_is_power_of_2(len));
  ccrw2_become_reader(FFT_CACHE_CCRW);
  /* An RDFT may need to initialize the cosine table after a CDFT. */
  if (len > FFT_LEN || len > (LSX_FFT_BR[0] << 2) || len > (LSX_FFT_BR[1] << 2)) {
    ccrw2_cease_reading(FFT_CACHE_CCRW);
    ccrw2_become_writer(FFT_CACHE_CCRW);
    if (len > FFT_LEN) {
      int old_n = FFT_LEN;
      FFT_LEN = len;
      LSX_FFT_BR = realloc(LSX_FFT_BR, dft_br_len(FFT_LEN) * sizeof(*LSX_FFT_BR));
      LSX_FFT_SC = realloc(LSX_FFT_SC, dft_sc_len(FFT_LEN) * sizeof(*LSX_FFT_SC));
      if (!old_n) {
        LSX_FFT_BR[0] = LSX_FFT_BR[1] = 0;
#if SOXR_LIB
        atexit(LSX_CLEAR_FFT_CACHE);
#endif
      }
    }
    /* Keep the writer lock until the transform has initialized the tables. */
    return true;
  }
  return false;
}

static void DONE_WITH_FFT_CACHE(bool is_writer)
{
  if (is_writer)
    ccrw2_cease_writing(FFT_CACHE_CCRW);
  else ccrw2_cease_reading(FFT_CACHE_CCRW);
}

void LSX_SAFE_RDFT(int len, int type, DFT_FLOAT * d)
{
  bool is_writer = UPDATE_FFT_CACHE(len);
  LSX_RDFT(len, type, d, LSX_FFT_BR, LSX_FFT_SC);
  DONE_WITH_FFT_CACHE(is_writer);
}

void LSX_SAFE_CDFT(int len, int type, DFT_FLOAT * d)
{
  bool is_writer = UPDATE_FFT_CACHE(len);
  LSX_CDFT(len, type, d, LSX_FFT_BR, LSX_FFT_SC);
  DONE_WITH_FFT_CACHE(is_writer);
}

#undef UPDATE_FFT_CACHE
#undef LSX_SAFE_RDFT
#undef LSX_SAFE_CDFT
#undef LSX_RDFT
#undef LSX_INIT_FFT_CACHE
#undef LSX_FFT_SC
#undef LSX_FFT_BR
#undef LSX_CLEAR_FFT_CACHE
#undef LSX_CDFT
#undef FFT_LEN
#undef FFT_CACHE_CCRW
#undef DONE_WITH_FFT_CACHE
#undef DFT_FLOAT
