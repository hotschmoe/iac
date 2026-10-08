/// One resource line in the UI: current stock and production per tick.
/// (The server sends no storage cap, so there is none here.)
class ResourceStock {
  final int amount;
  final double rate;

  const ResourceStock({required this.amount, required this.rate});

  ResourceStock tick() => ResourceStock(amount: amount + rate.round(), rate: rate);
}

class Resources {
  final ResourceStock metal;
  final ResourceStock crystal;
  final ResourceStock deut;

  const Resources({
    required this.metal,
    required this.crystal,
    required this.deut,
  });
}
