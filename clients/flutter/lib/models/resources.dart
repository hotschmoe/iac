class ResourceStock {
  final int amount;
  final int rate;
  final int cap;

  const ResourceStock({
    required this.amount,
    required this.rate,
    required this.cap,
  });

  double get fraction => cap > 0 ? amount / cap : 0;

  ResourceStock tick() => ResourceStock(
        amount: (amount + rate).clamp(0, cap),
        rate: rate,
        cap: cap,
      );

  ResourceStock copyWith({int? amount, int? rate, int? cap}) => ResourceStock(
        amount: amount ?? this.amount,
        rate: rate ?? this.rate,
        cap: cap ?? this.cap,
      );
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

  Resources tick() => Resources(
        metal: metal.tick(),
        crystal: crystal.tick(),
        deut: deut.tick(),
      );

  Resources copyWith({
    ResourceStock? metal,
    ResourceStock? crystal,
    ResourceStock? deut,
  }) =>
      Resources(
        metal: metal ?? this.metal,
        crystal: crystal ?? this.crystal,
        deut: deut ?? this.deut,
      );
}
