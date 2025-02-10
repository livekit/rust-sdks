#ifndef ANY_VPL_SESSION_IMPL_H_
#define ANY_VPL_SESSION_IMPL_H_

#include <mfxdispatcher.h>
#include <mfxvideo++.h>
#include <va/va.h>
#include <iostream>
#include <memory>
#include <mutex>

namespace any_vpl {

/**
 * @brief Wraps an Intel® VPL session.
 * For now it is a singleton, because having one session can reduce the overhead of creating a session per encoder and context switching on
 * the GPU.
 *
 */
class VplSessionSingleton {
 public:
  /**
   * @brief Get the singleton instance of VplSession
   *
   * @return std::shared_ptr<VplSessionSingleton> A pointer to the singleton instance
   */
  static std::shared_ptr<VplSessionSingleton> Instance();

  ~VplSessionSingleton();

  // Delete copy constructor and assignment operator to prevent copies
  VplSessionSingleton(const VplSessionSingleton&) = delete;
  VplSessionSingleton& operator=(const VplSessionSingleton&) = delete;

  /**
   * @brief Get the Vpl Session
   *
   * @return mfxSession The Vpl Session
   */
  mfxSession GetVplSession() const;

 private:
  static std::shared_ptr<VplSessionSingleton> instance_;
  static std::mutex mutex_;

  mfxLoader loader_{nullptr};
  mfxSession session_{nullptr};
  int accelratorFD_{0};
  VADisplay vaDisplay_{nullptr};

  /**
   * @brief Private constructor to prevent direct instantiation
   *
   */
  VplSessionSingleton() = default;

  /**
   * @brief Handles all the required initializations for the VPL session.
   * MFXLoad, which enumerates and initializes all available runtimes
   * MFXCreateSession, which creates a session for the selected runtime
   * MFXQueryIMPL, returns the implementation type of a given session
   *
   */
  bool Create();

  /**
   * @brief If the hardware acceleration goes through the Linux* VA-API infrastructure, this function initializes the VA-API context and
   * sets the session handle.
   *
   * @param implementation The implementation type
   */
  void InitAcceleratorHandle(mfxIMPL implementation);

  /**
   * @brief Shows implementation info with Intel® VPL
   *
   * @param implnum
   */
  void ShowImplementationInfo(mfxU32 implnum);
};

}  // namespace any_vpl

#endif