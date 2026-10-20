//! `consume` against the NATS of `just up`: handlers run in parallel, `stop` waits for the
//! messages in flight, and a handler that panics does not take the consumer down.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use async_nats::jetstream::consumer::PullConsumer;
use taut_bus::{Bus, CHECKS_STREAM, Delivery, Handled};
use taut_config::Settings;
use tokio::sync::watch;

async fn bus() -> Bus {
    let settings = Settings::load().expect("settings");
    Bus::connect(&settings.nats, "bus-tests")
        .await
        .expect("nats is up (just up)")
}

/// A subject of the `CHECKS` stream that only this test uses, and a durable consumer on it.
async fn consumer(bus: &Bus, test: &str, ack_wait: Duration) -> (String, PullConsumer) {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let name = format!("bus-test-{test}-{nanos}");
    let subject = format!("checks.test.{name}");
    let consumer = bus
        .durable_consumer(CHECKS_STREAM, &name, &subject, ack_wait)
        .await
        .unwrap();
    (subject, consumer)
}

/// Waits until `counter` reaches `target`, or panics after five seconds.
async fn wait_for(counter: &AtomicUsize, target: usize) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while counter.load(Ordering::SeqCst) < target {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("handlers did not run in time");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn handlers_run_in_parallel_and_stop_waits_for_them() {
    const MESSAGES: usize = 3;
    const WORK: Duration = Duration::from_millis(400);
    let bus = bus().await;
    let (subject, consumer) = consumer(&bus, "parallel", Duration::from_secs(30)).await;
    for n in 0..MESSAGES {
        bus.publish(&subject, "trace-bus", &n).await.unwrap();
    }

    let started = Arc::new(AtomicUsize::new(0));
    let finished = Arc::new(AtomicUsize::new(0));
    let (stop_tx, stop_rx) = watch::channel(false);
    let began = Instant::now();
    let worker = tokio::spawn({
        let (started, finished) = (Arc::clone(&started), Arc::clone(&finished));
        taut_bus::consume(consumer, MESSAGES, stop_rx, move |_: Delivery<usize>| {
            let (started, finished) = (Arc::clone(&started), Arc::clone(&finished));
            async move {
                started.fetch_add(1, Ordering::SeqCst);
                // Blocks its thread, like CPU-bound work: handlers that share one task would
                // take MESSAGES × WORK.
                std::thread::sleep(WORK);
                finished.fetch_add(1, Ordering::SeqCst);
                Handled::Done
            }
        })
    });

    wait_for(&started, MESSAGES).await;
    stop_tx.send(true).unwrap();
    worker.await.unwrap().unwrap();
    let elapsed = began.elapsed();

    assert_eq!(
        finished.load(Ordering::SeqCst),
        MESSAGES,
        "stop must wait for the messages in flight"
    );
    assert!(
        elapsed < WORK * 2,
        "handlers ran one after another: {elapsed:?} for {MESSAGES} × {WORK:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_panicking_handler_loses_only_its_message_until_redelivery() {
    let bus = bus().await;
    let (subject, consumer) = consumer(&bus, "panic", Duration::from_secs(1)).await;
    bus.publish(&subject, "trace-bus", &1_u32).await.unwrap();
    bus.publish(&subject, "trace-bus", &2_u32).await.unwrap();

    let panicked = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicUsize::new(0));
    let (stop_tx, stop_rx) = watch::channel(false);
    let worker = tokio::spawn({
        let (panicked, done) = (Arc::clone(&panicked), Arc::clone(&done));
        taut_bus::consume(consumer, 4, stop_rx, move |delivery: Delivery<u32>| {
            let (panicked, done) = (Arc::clone(&panicked), Arc::clone(&done));
            async move {
                let first_delivery = delivery.event == 1 && !panicked.swap(true, Ordering::SeqCst);
                assert!(!first_delivery, "handler bug on the first delivery");
                done.fetch_add(1, Ordering::SeqCst);
                Handled::Done
            }
        })
    });

    // Message 2 is handled right away; message 1 comes back after the 1 s ack wait.
    wait_for(&done, 2).await;
    assert!(panicked.load(Ordering::SeqCst));
    assert!(!worker.is_finished(), "the consumer must survive the panic");
    stop_tx.send(true).unwrap();
    worker.await.unwrap().unwrap();
}
