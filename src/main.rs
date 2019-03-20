use std::{fs::File, str::FromStr};

use num::Complex;

use image::{png::PNGEncoder, ColorType};

fn main() {
  let args: Vec<String> = std::env::args().collect();

  if args.len() != 6 {
    eprintln!("Usage: mandelbrot FILE PIXELS UPPERLEFT LOWERRIGHT THREADS");
    eprintln!(
      "Example: {} mandel.png 1000x750 -1.20,0.35 -1,0.20 8",
      args[0]
    );

    std::process::exit(1);
  }

  let bounds = parse_pair(&args[2], 'x').expect("error parsing image dimensions");
  let upper_left = parse_complex(&args[3]).expect("error parsing upper left value");
  let lower_right = parse_complex(&args[4]).expect("error parsing lower right value");

  let mut pixels = vec![0; bounds.0 * bounds.1];

  let threads = usize::from_str(&args[5]).expect("error parsing number of threads");
  let rows_per_band = bounds.1 / threads + 1;

  {
    let bands: Vec<&mut [u8]> = pixels.chunks_mut(rows_per_band * bounds.0).collect();
    crossbeam::scope(|spawner| {
      for (i, band) in bands.into_iter().enumerate() {
        let top = rows_per_band * i;
        let height = band.len() / bounds.0;

        let band_bounds = (bounds.0, height);
        let band_upper_left = pixel_to_point(bounds, (0, top), upper_left, lower_right);
        let band_lower_right =
          pixel_to_point(bounds, (bounds.0, top + height), upper_left, lower_right);

        spawner.spawn(move |_| {
          render(band, band_bounds, band_upper_left, band_lower_right);
        });
      }
    })
    .expect("failed to compute bands");
  }

  write_image(&args[1], &pixels, bounds).expect("error writing PNG file");
}

/// Parse the string `target` as a coordinate pair, like `"400x600"` or `"1.0,0.5"`.
/// Specifically, `target` should have the form <left><sep><right>, where <sep> is
/// the character given by the `separator` argument, and <left> and <right> are both
/// strings that can be parsed by `T::from_str`.
///
/// If `target` has the proper form, return `Some<(x, y)>`. If it doesn't parse
/// correctly, return `None`.
fn parse_pair<T: FromStr>(target: &str, separator: char) -> Option<(T, T)> {
  match target.find(separator) {
    None => None,
    Some(index) => {
      match (
        T::from_str(&target[..index]),
        T::from_str(&target[index + 1..]),
      ) {
        (Ok(left), Ok(right)) => Some((left, right)),
        _ => None,
      }
    }
  }
}

#[test]
fn test_parse_pair() {
  assert_eq!(parse_pair::<i32>("", ','), None);
  assert_eq!(parse_pair::<i32>("10,", ','), None);
  assert_eq!(parse_pair::<i32>(",10", ','), None);
  assert_eq!(parse_pair::<i32>("10,20", ','), Some((10, 20)));
  assert_eq!(parse_pair::<i32>("10,20xy", ','), None);
  assert_eq!(parse_pair::<f64>("0.5x", 'x'), None);
  assert_eq!(parse_pair::<f64>("0.5x1.5", 'x'), Some((0.5, 1.5)));
}

/// Try to parse a `target` into a `Complex<f64>`.
///
/// Expects a `target` in the `"re,im"` format.
fn parse_complex(target: &str) -> Option<Complex<f64>> {
  let (re, im) = parse_pair(target, ',')?;

  Some(Complex { re, im })
}

#[test]
fn test_parse_complex() {
  assert_eq!(
    parse_complex("1.25,-0.0625"),
    Some(Complex {
      re: 1.25,
      im: -0.0625
    })
  );

  assert_eq!(parse_complex(",1.25"), None);
}

/// Try to determine if `target` is in the Mandelbrot set, using at most `limit`
/// iterations to decide.
///
/// If `target` is not a member, return `Some(i)`, where `i` is the number of
/// iterations it took for `target` to leave the circle of radius two centered on the
/// origin. If `target` seems to be a member (more precisely, if we reached the
/// iteration limit without being able to prove that `target` is not a member),
/// return `None`.
fn escape_time(target: Complex<f64>, limit: u32) -> Option<u32> {
  let mut accumulator = Complex { re: 0.0, im: 0.0 };
  for i in 0..limit {
    accumulator = accumulator * accumulator + target;
    if accumulator.norm_sqr() > 4.0 {
      return Some(i);
    }
  }

  None
}

/// Given the row and column of a pixel in the output image, return the
/// corresponding point on the complex plane.
///
/// `bounds` is a pair giving the width and height of the image in pixels.
/// `pixel` is a (column, row) pair indicating a particular pixel in that image.
/// The `upper_left` and `lower_right` parameters are points on the complex
/// plane designating the area our image covers.
fn pixel_to_point(
  bounds: (usize, usize),
  pixel: (usize, usize),
  upper_left: Complex<f64>,
  lower_right: Complex<f64>,
) -> Complex<f64> {
  let (width, height) = (
    lower_right.re - upper_left.re,
    upper_left.im - lower_right.im,
  );

  Complex {
    re: upper_left.re + pixel.0 as f64 * width / bounds.0 as f64,
    im: upper_left.im - pixel.1 as f64 * height / bounds.1 as f64,
  }
}

#[test]
fn test_pixel_to_point() {
  assert_eq!(
    pixel_to_point(
      (100, 100),
      (25, 75),
      Complex { re: -1.0, im: 1.0 },
      Complex { re: 1.0, im: -1.0 }
    ),
    Complex { re: -0.5, im: -0.5 }
  );
}

/// Render a rectangle of the Mandelbrot set into a buffer of pixels.
///
/// The `bounds` argument gives the width and height of the buffer `pixels`,
/// which holds one grayscale pixel per byte. The `upper_left` and `lower_right`
/// arguments specify points on the complex plane corresponding to the upper-
/// left and lower-right corners of the pixel buffer.
fn render(
  pixels: &mut [u8],
  bounds: (usize, usize),
  upper_left: Complex<f64>,
  lower_right: Complex<f64>,
) {
  assert!(pixels.len() == bounds.0 * bounds.1);

  for row in 0..bounds.1 {
    for column in 0..bounds.0 {
      let point = pixel_to_point(bounds, (column, row), upper_left, lower_right);

      pixels[row * bounds.0 + column] = match escape_time(point, 255) {
        None => 0,
        Some(count) => 255 - count as u8,
      }
    }
  }
}

/// Write the buffer `pixels`, whose dimensions are given by `bounds`, to the
/// file named `filename`.
fn write_image(
  filename: &str,
  pixels: &[u8],
  bounds: (usize, usize),
) -> Result<(), std::io::Error> {
  let output = File::create(filename)?;

  let encoder = PNGEncoder::new(output);

  encoder.encode(
    &pixels,
    bounds.0 as u32,
    bounds.1 as u32,
    ColorType::Gray(8),
  )?;

  Ok(())
}
