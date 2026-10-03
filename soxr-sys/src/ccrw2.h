/* SoX Resampler Library      Copyright (c) 2007-13 robs@users.sourceforge.net
 * Licence for this file: LGPL v2.1                  See LICENCE for details. */

#if !defined soxr_ccrw2_included
#define soxr_ccrw2_included

#if defined SOXR_LIB
#include "internal.h"
#endif

/* The FFT caches are shared even when OpenMP is disabled.  Static lock
 * initialization also makes concurrent first use safe. */
#if defined _WIN32
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>

typedef SRWLOCK ccrw2_t;
#define CCRW2_INITIALIZER SRWLOCK_INIT
#define ccrw2_become_reader(p) AcquireSRWLockShared(&(p))
#define ccrw2_cease_reading(p) ReleaseSRWLockShared(&(p))
#define ccrw2_become_writer(p) AcquireSRWLockExclusive(&(p))
#define ccrw2_cease_writing(p) ReleaseSRWLockExclusive(&(p))
#else
#include <pthread.h>

typedef pthread_rwlock_t ccrw2_t;
#define CCRW2_INITIALIZER PTHREAD_RWLOCK_INITIALIZER
#define ccrw2_become_reader(p) (void)pthread_rwlock_rdlock(&(p))
#define ccrw2_cease_reading(p) (void)pthread_rwlock_unlock(&(p))
#define ccrw2_become_writer(p) (void)pthread_rwlock_wrlock(&(p))
#define ccrw2_cease_writing(p) (void)pthread_rwlock_unlock(&(p))
#endif

#endif
