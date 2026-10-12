// The wire protocol as the client uses it.
//
// The types themselves, their JSON, their defaults, the labels and the
// constants Rust holds are in protocol.g.dart, which is generated from
// shared/src (see its header) and is never edited. This file holds what the
// client adds to them. Nothing here names a wire key or a tag.

import 'dart:typed_data';

import 'hex.dart';
import 'protocol.g.dart';

export 'hex.dart';
export 'protocol.g.dart';

/// Whether [name] is a built-in bot's.
bool isBotName(String name) => name.toUpperCase().startsWith(botNamePrefix.toUpperCase());

/// The band of a ratio, as the server's `scaling::ratio_label` cuts it. The
/// server holds a ratio as an `f32`, so [ratio] is rounded to one first and
/// the edges fall where the server's do. The ratio itself only the server
/// can work out; the demo world uses this to label the ones it makes up.
RatioLabel ratioLabelFor(double ratio) {
  final r = (Float32List(1)..[0] = ratio)[0];
  if (r >= ratioSafe) return RatioLabel.safe;
  if (r >= ratioFavourable) return RatioLabel.favourable;
  if (r >= ratioEven) return RatioLabel.even;
  if (r >= ratioRisky) return RatioLabel.risky;
  return RatioLabel.deadly;
}

extension BatchWaitReason on BatchWait {
  WaitReason get reason => switch (this) {
        BatchWait.slot => WaitReason.slot,
        BatchWait.prerequisite => WaitReason.prerequisite,
      };
}

extension ShipyardItemLabel on ShipyardItem {
  String get label => ship?.label ?? defence!.label;
}

extension ResourceKindOf on ResourceKind {
  double of(Resources r) => switch (this) {
        ResourceKind.metal => r.metal,
        ResourceKind.crystal => r.crystal,
        ResourceKind.deuterium => r.deuterium,
      };
}

extension ResourcesTotal on Resources {
  double get total => metal + crystal + deuterium;
}

extension ResourceEtaOf on ResourceEta {
  int? of(ResourceKind k) => switch (k) {
        ResourceKind.metal => metal,
        ResourceKind.crystal => crystal,
        ResourceKind.deuterium => deuterium,
      };
}

extension WorldInfoHub on WorldInfo {
  /// What the hub offers at [at]; nothing anywhere else.
  List<HubService> servicesAt(Hex at) {
    for (final site in hub) {
      if (site.location == at) return site.services;
    }
    return const [];
  }

  /// The hub sector nearest [from] that offers [service].
  Hex? nearest(HubService service, Hex from) {
    Hex? best;
    for (final site in hub) {
      if (!site.services.contains(service)) continue;
      if (best == null || Hex.distance(site.location, from) < Hex.distance(best, from)) best = site.location;
    }
    return best;
  }

  /// "x600 blitz" or "x42".
  String get label {
    final n = pace == pace.roundToDouble() ? pace.round().toString() : pace.toString();
    return preset == null ? 'x$n' : 'x$n $preset';
  }
}

extension ExchangeMeterFill on ExchangeMeter {
  /// How full the meter is, 0 to 1; full when it has no span.
  double get fill => span > 0 ? (pressure / span).clamp(0, 1).toDouble() : 1;
}

extension MovePreviewOdds on MovePreview {
  /// The odds part of the preview.
  SectorOdds get odds => SectorOdds(sector: target, threat: threat, ratio: ratio, label: label);
}

extension NpcFleetInfoCount on NpcFleetInfo {
  int get shipCount => ships.fold(0, (s, e) => s + e.count);
}

extension RequirementLabel on Requirement {
  /// As cards and captions print it, in the engine's words: "Shipyard level 4".
  String get label => '$name level $need';
}

extension HomeworldStateEdit on HomeworldState {
  int buildingLevel(BuildingType t) {
    for (final b in buildings) {
      if (b.buildingType == t) return b.level;
    }
    return 0;
  }

  /// A copy with some fields replaced (for tests and the offline demo).
  HomeworldState copyWith({
    Resources? production,
    StorageState? storage,
    List<BuildingState>? buildings,
    List<ResearchState>? research,
    List<DefenceState>? defences,
    double? homeDefencePower,
    List<BuildQueueItem>? buildQueue,
    List<QueuedBuild>? buildPending,
    int? buildSlots,
    ShipyardQueueItem? shipyardQueue,
    bool clearShipyardQueue = false,
    List<QueuedShip>? shipyardPending,
    ResearchItem? researchActive,
    bool clearResearchActive = false,
    List<QueuedResearch>? researchPending,
    HomeworldCatalog? catalog,
  }) =>
      HomeworldState(
        location: location,
        production: production ?? this.production,
        storage: storage ?? this.storage,
        buildings: buildings ?? this.buildings,
        research: research ?? this.research,
        buildQueue: buildQueue ?? this.buildQueue,
        buildPending: buildPending ?? this.buildPending,
        buildSlots: buildSlots ?? this.buildSlots,
        queueWaitingMax: queueWaitingMax,
        shipyardQueue: clearShipyardQueue ? null : (shipyardQueue ?? this.shipyardQueue),
        shipyardPending: shipyardPending ?? this.shipyardPending,
        researchActive: clearResearchActive ? null : (researchActive ?? this.researchActive),
        researchPending: researchPending ?? this.researchPending,
        dockedShips: dockedShips,
        defences: defences ?? this.defences,
        homeDefencePower: homeDefencePower ?? this.homeDefencePower,
        nextRaidEstimatePower: nextRaidEstimatePower,
        catalog: catalog ?? this.catalog,
      );
}
