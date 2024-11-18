#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use embedded_hal::delay;
use esp_backtrace as _;
use esp_hal::{
    delay::Delay, gpio::{Flex, InputPin, Io, Level, Output, OutputPin, Pull}, prelude::*
};
use log::{info, error};
use ds18b20::{Resolution, Ds18b20};
use one_wire_bus::{OneWire, OneWireResult, OneWireError};

extern crate alloc;

#[main]
async fn main(spawner: Spawner) {
    let peripherals = esp_hal::init({
        let mut config = esp_hal::Config::default();
        config.cpu_clock = CpuClock::max();
        config
    });

    let io = Io::new(peripherals.GPIO, peripherals.IO_MUX);

    let mut sensor_pin = Flex::new(io.pins.gpio4);
    let mut led_pin = Output::new(io.pins.gpio2, Level::High);

    esp_alloc::heap_allocator!(72 * 1024);

    esp_println::logger::init_logger_from_env();

    let mut one_wire_bus = OneWire::new(sensor_pin).unwrap();

    let mut delay = Delay::new();

    let timg0 = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG0);
    esp_hal_embassy::init(timg0.timer0);
    info!("Embassy initialized!");

    let timg1 = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG1);
    let _init = esp_wifi::init(
        esp_wifi::EspWifiInitFor::Wifi,
        timg1.timer0,
        esp_hal::rng::Rng::new(peripherals.RNG),
        peripherals.RADIO_CLK,
    )
    .unwrap();

    // TODO: Spawn some tasks
    let _ = spawner;

    info!("Hello world, starting temperature reading loop");

    loop {
        led_pin.set_high();
        get_temperature(&mut delay, &mut one_wire_bus).unwrap();
        led_pin.set_low();
        Timer::after(Duration::from_secs(1)).await;
    }
}


fn get_temperature<P>(
    delay: &mut Delay,
    one_wire_bus: &mut OneWire<P>,
) -> OneWireResult<(), P::Error>
    where
        P: embedded_hal::digital::OutputPin + embedded_hal::digital::InputPin,
{
    info!("Starting temperature measurement");
    // initiate a temperature measurement for all connected devices
    match ds18b20::start_simultaneous_temp_measurement(one_wire_bus, delay) {
        Ok(_) => (),
        Err(e) => {
            error!("Error starting temperature measurement: {:?}", e);
            return Err(e);
        }
    }

    // wait until the measurement is done. This depends on the resolution you specified
    // If you don't know the resolution, you can obtain it from reading the sensor data,
    // or just wait the longest time, which is the 12-bit resolution (750ms)
    Resolution::Bits12.delay_for_measurement_time(delay);

    
    // iterate over all the devices, and report their temperature
    let mut search_state = None;
    loop {
        info!("Reading temperature from devices");

        if let Some((device_address, state)) = one_wire_bus.device_search(search_state.as_ref(), false, delay)? {
            search_state = Some(state);
            if device_address.family_code() != ds18b20::FAMILY_CODE {
                info!("Skipping device {:?} with family code {:?}", device_address, device_address.family_code());
                continue;
            }
            // You will generally create the sensor once, and save it for later
            let sensor = Ds18b20::new(device_address)?;

            // contains the read temperature, as well as config info such as the resolution used
            let sensor_data = sensor.read_data(one_wire_bus, delay)?;
            info!("Device at {:?} is {}°C", device_address, sensor_data.temperature);
        } else {
            error!("No more devices found");
            break;
        }
    }
    Ok(())
}