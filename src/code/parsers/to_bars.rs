// to-bars: any rows into price bars (timestamp, open, high, low, close, volume), one
// shape among many. Columns are found by name: timestamp/ts/time/date, open/high/low/close,
// or one price/px/last column (then open = high = low = close), and volume/vol/qty/size.
// A built-in example: copy it (`mnnn code create my-bars --from to-bars`) and change it.
use muninn_plugin::prelude::*;

fn find<'a>(f: &Frame, names: &[&'a str]) -> Option<&'a str> {
    names.iter().copied().find(|n| f.has(n))
}

pub fn run(host: &Host) -> Result<View> {
    let f = host.input()?;
    let ts = find(&f, &["timestamp", "ts", "time", "date", "_created_at"]).ok_or_else(|| Error::new("no timestamp/ts/time/date column"))?;
    let price = find(&f, &["close", "price", "px", "last"]).ok_or_else(|| Error::new("no close/price/px/last column"))?;
    let col = |n: Option<&str>| -> Result<Vec<f64>> {
        match n {
            Some(n) => f.f64(n),
            None => f.f64(price),
        }
    };
    let (t, close) = (f.f64(ts)?, f.f64(price)?);
    let (open, high, low) = (col(find(&f, &["open"]))?, col(find(&f, &["high"]))?, col(find(&f, &["low"]))?);
    let vol = match find(&f, &["volume", "vol", "qty", "size"]) {
        Some(v) => f.f64(v)?,
        None => vec![0.0; f.len()],
    };
    let mut view = View::table("Bars", &["timestamp", "open", "high", "low", "close", "volume"]);
    for i in 0..f.len() {
        view = view.row(vec![t[i].into(), open[i].into(), high[i].into(), low[i].into(), close[i].into(), vol[i].into()]);
    }
    host.log(format!("{} bars from {ts} and {price}", f.len()));
    Ok(view)
}
