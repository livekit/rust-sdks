use webrtc_sys::{frame_transformer as sys_ft};
use futures::stream::Stream;
use tokio::sync::mpsc;
use cxx::{SharedPtr, UniquePtr};
use crate::sender_report::SenderReport;
use webrtc_sys::sender_report::ffi::SenderReport as sys_sr;
use crate::prelude::RtpReceiver;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct NativeSenderReportStream {
    native_callback: SharedPtr<sys_ft::ffi::AdaptedNativeSenderReportCallback>,
    _observer: Box<SenderReportsObserver>,
    sr_rx: mpsc::UnboundedReceiver<SenderReport>,
}

impl NativeSenderReportStream {
    pub fn new(rtp_receiver: &RtpReceiver) -> Self {
        let (sr_tx, sr_rx) = mpsc::unbounded_channel();
        let mut observer = Box::new(SenderReportsObserver { sr_tx });
        let mut native_callback = unsafe {
            sys_ft::ffi::new_adapted_sender_report_callback(Box::new(sys_ft::SenderReportSinkWrapper::new(
                &mut *observer
            )))
        };

        rtp_receiver.set_sender_report_callback(native_callback.clone());

        Self {
            native_callback: native_callback,
            _observer: observer,
            sr_rx
        }
    }

    pub fn close(&mut self) {
        self.sr_rx.close();
    }
}

impl Drop for NativeSenderReportStream {
    fn drop(&mut self) {
        self.close();
    }
}

impl Stream for NativeSenderReportStream {
    type Item = SenderReport;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.sr_rx.poll_recv(cx)
    }
}

struct SenderReportsObserver {
    sr_tx: mpsc::UnboundedSender<SenderReport>,
}

impl sys_ft::SenderReportSink for SenderReportsObserver {
    // To be called when Transform happens
    fn on_sender_report(&self, sender_report: UniquePtr<sys_sr>) {
        println!("SenderReportsObserver::on_sender_report");
        let sender_report = SenderReport::new(sender_report);
        let _ = self.sr_tx.send(sender_report);
    }
}
