use super::entities::PlayerMetaData;
use super::variants::Variant;
use crate::parser_settings::Parser;
use crate::variants::PropColumn;
use phf_macros::phf_map;
use soa_derive::StructOfArray;

#[derive(Debug, StructOfArray)]
pub struct ProjectileRecord {
    pub steamid: Option<u64>,
    pub name: Option<String>,
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub z: Option<f32>,
    pub tick: Option<i32>,
    pub grenade_type: Option<String>,
}

// This file collects the data that is converted into a dataframe in the end in parser.parse_ticks()

impl<'a> Parser<'a> {
    pub fn collect_entities(&mut self) {
        if !self.wanted_ticks.contains(&self.tick) && self.wanted_ticks.len() != 0 {
            return;
        }
        if self.parse_projectiles {
            self.collect_projectiles();
        }
        // iterate every player and every wanted prop name
        // if either one is missing then push None to output
        for (entity_id, player) in &self.players {
            for prop_name in &self.wanted_player_props {
                // returns none if missing
                let prop = self.find_prop(prop_name, entity_id, player);
                self.output
                    .entry(prop_name.to_string())
                    .or_insert_with(|| PropColumn::new())
                    .push(prop);
            }
        }
    }
    pub fn get_prop_for_ent(&self, prop_name: &str, entity_id: &i32) -> Option<Variant> {
        // Function that allows you to use string name for the prop and the function
        // translates it to a path. This costs a bit but the elegance of using string names
        // is just too big to give up. Also I think paths change between demos soo...
        match &self.prop_name_to_path.get(prop_name) {
            Some(path) => {
                if let Some(ent) = self.entities.get(&entity_id) {
                    if let Some(prop) = ent.props.get(path.clone()) {
                        return Some(prop.clone());
                    }
                }
            }
            None => return None,
        }
        None
    }
    pub fn find_prop(
        &self,
        prop_name: &str,
        entity_id: &i32,
        player: &PlayerMetaData,
    ) -> Option<Variant> {
        // Early exit these metadata props
        match prop_name {
            "tick" => return Some(Variant::I32(self.tick)),
            "steamid" => match player.steamid {
                Some(steamid) => return Some(Variant::U64(steamid)),
                _ => return None,
            },
            "name" => match &player.name {
                Some(name) => return Some(Variant::String(name.to_string())),
                _ => return None,
            },
            _ => {}
        }
        match TYPEHM.get(prop_name) {
            Some(PropType::Team) => return self.find_team_prop(entity_id, prop_name),
            Some(PropType::Custom) => return self.create_custom_prop(&prop_name, entity_id),
            Some(PropType::Controller) => match player.controller_entid {
                Some(entid) => return self.get_prop_for_ent(prop_name, &entid),
                None => return None,
            },
            Some(PropType::Rules) => match self.rules_entity_id {
                Some(rules_entid) => return self.get_prop_for_ent(prop_name, &rules_entid),
                None => return None,
            },
            Some(PropType::Player) => {
                if let Some(e) = player.controller_entid {
                    let is_alive = self.get_prop_for_ent("CCSPlayerController.m_bPawnIsAlive", &e);
                    match is_alive {
                        Some(Variant::Bool(true)) => {
                            return self.get_prop_for_ent(&prop_name, &entity_id)
                        }
                        _ => return None,
                    }
                };
            }
            _ => return None,
        }
        None
    }
    pub fn collect_cell_coordinate_player(&self, axis: &str, entity_id: &i32) -> Option<Variant> {
        let offset = self.get_prop_for_ent(
            &("CCSPlayerPawn.CBodyComponentBaseAnimGraph.m_vec".to_owned() + axis),
            entity_id,
        );
        let cell = self.get_prop_for_ent(
            &("CCSPlayerPawn.CBodyComponentBaseAnimGraph.m_cell".to_owned() + axis),
            entity_id,
        );
        if let Some(coord) = coord_from_cell(cell, offset) {
            return Some(Variant::F32(coord));
        }
        None
    }
    pub fn collect_cell_coordinate_grenade(&self, axis: &str, entity_id: &i32) -> Option<Variant> {
        let entity = match self.entities.get(entity_id) {
            Some(ent) => ent,
            None => return None,
        };
        let class = match self.cls_by_id[entity.cls_id as usize].as_ref() {
            Some(cls) => cls,
            None => return None,
        };
        let offset = self.get_prop_for_ent(
            &(class.name.to_owned() + "." + "CBodyComponentBaseAnimGraph.m_vec" + axis),
            entity_id,
        );
        let cell = self.get_prop_for_ent(
            &(class.name.to_owned() + "." + "CBodyComponentBaseAnimGraph.m_cell" + axis),
            entity_id,
        );
        if let Some(coord) = coord_from_cell(cell, offset) {
            return Some(Variant::F32(coord));
        }
        None
    }
    pub fn find_thrower_steamid(&self, entity_id: &i32) -> Option<u64> {
        let entity = match self.entities.get(entity_id) {
            Some(ent) => ent,
            None => return None,
        };
        let class = match self.cls_by_id[entity.cls_id as usize].as_ref() {
            Some(cls) => cls,
            None => return None,
        };
        let owner_entid =
            match self.get_prop_for_ent(&(class.name.to_owned() + "." + "m_nOwnerId"), entity_id) {
                Some(Variant::U32(prop)) => Some(prop & 0x7FF),
                _ => None,
            };
        let steamid = match owner_entid {
            Some(entid) => match self.players.get(&(entid as i32)) {
                Some(metadata) => metadata.steamid,
                None => None,
            },
            None => None,
        };
        steamid
    }
    pub fn find_thrower_name(&self, entity_id: &i32) -> Option<String> {
        let entity = match self.entities.get(entity_id) {
            Some(ent) => ent,
            None => return None,
        };
        let class = match self.cls_by_id[entity.cls_id as usize].as_ref() {
            Some(cls) => cls,
            None => return None,
        };
        let owner_entid =
            match self.get_prop_for_ent(&(class.name.to_owned() + "." + "m_nOwnerId"), entity_id) {
                Some(Variant::U32(prop)) => Some(prop & 0x7FF),
                _ => None,
            };
        let name = match owner_entid {
            Some(entid) => match self.players.get(&(entid as i32)) {
                Some(metadata) => return metadata.name.clone(),
                None => None,
            },
            None => None,
        };
        name
    }
    fn find_grenade_type(&self, entity_id: &i32) -> Option<String> {
        if let Some(ent) = self.entities.get(&entity_id) {
            if let Some(cls) = self.cls_by_id[ent.cls_id as usize].as_ref() {
                // remove extra from name: CSmokeGrenadeProjectile --> SmokeGrenade
                // Todo maybe make names like this: smoke_grenade or just "smoke"
                let mut clean_name = cls.name[1..].split_at(cls.name.len() - 11).0;
                // Seems like the only exception
                if clean_name == "BaseCSGrenade" {
                    clean_name = "HeGrenade"
                }
                return Some(clean_name.to_owned());
            }
        }
        None
    }
    pub fn collect_projectiles(&mut self) {
        for projectile_entid in &self.projectiles {
            let grenade_type = self.find_grenade_type(projectile_entid);
            let steamid = self.find_thrower_steamid(projectile_entid);
            let name = self.find_thrower_name(projectile_entid);
            let x = self.collect_cell_coordinate_grenade("X", projectile_entid);
            let y = self.collect_cell_coordinate_grenade("Y", projectile_entid);
            let z = self.collect_cell_coordinate_grenade("Z", projectile_entid);

            let float_x = match x {
                Some(Variant::F32(p)) => Some(p),
                _ => None,
            };
            let float_y = match y {
                Some(Variant::F32(p)) => Some(p),
                _ => None,
            };
            let float_z = match z {
                Some(Variant::F32(p)) => Some(p),
                _ => None,
            };

            self.projectile_records.push(ProjectileRecord {
                steamid: steamid,
                name: name,
                x: float_x,
                y: float_y,
                z: float_z,
                tick: Some(self.tick),
                grenade_type: grenade_type,
            });
        }
    }
    pub fn find_team_prop(&self, player_entid: &i32, prop: &str) -> Option<Variant> {
        if let Some(Variant::U32(team_num)) =
            self.get_prop_for_ent("CCSPlayerPawn.m_iTeamNum", player_entid)
        {
            let team_entid = match team_num {
                // 1 should be spectator
                1 => self.teams.team1_entid,
                2 => self.teams.team2_entid,
                3 => self.teams.team3_entid,
                _ => None,
            };
            // Get prop from team entity
            if let Some(entid) = team_entid {
                if let Some(p) = self.get_prop_for_ent(prop, &entid) {
                    return Some(p);
                }
            }
        }
        None
    }
    pub fn create_custom_prop(&self, prop_name: &str, entity_id: &i32) -> Option<Variant> {
        match prop_name {
            "X" => self.collect_cell_coordinate_player("X", entity_id),
            "Y" => self.collect_cell_coordinate_player("Y", entity_id),
            "Z" => self.collect_cell_coordinate_player("Z", entity_id),
            _ => panic!("unknown custom prop: {}", prop_name),
        }
    }
}
fn coord_from_cell(cell: Option<Variant>, offset: Option<Variant>) -> Option<f32> {
    // DONT KNOW IF THESE ARE CORRECT. SEEMS TO GIVE CORRECT VALUES
    let cell_bits = 9;
    let max_coord = (1 << 14) as f32;
    // Both are cell and offset are needed for calculation
    if let Some(Variant::U32(cell)) = cell {
        if let Some(Variant::F32(offset)) = offset {
            let cell_coord = ((cell as f32 * (1 << cell_bits) as f32) - max_coord) as f32;
            return Some(cell_coord + offset);
        }
    }
    None
}
pub enum PropType {
    Team,
    Rules,
    Custom,
    Controller,
    Player,
}

pub static TYPEHM: phf::Map<&'static str, PropType> = phf_map! {
    // TEAM
    "CCSTeam.m_iTeamNum" => PropType::Team,
    "CCSTeam.m_aPlayers" => PropType::Team,
    "CCSTeam.m_aPawns" => PropType::Team,
    "CCSTeam.m_iScore" => PropType::Team,
    "CCSTeam.m_szTeamname" => PropType::Team,
    "CCSTeam.m_bSurrendered" => PropType::Team,
    "CCSTeam.m_szTeamMatchStat" => PropType::Team,
    "CCSTeam.m_numMapVictories" => PropType::Team,
    "CCSTeam.m_scoreFirstHalf" => PropType::Team,
    "CCSTeam.m_scoreSecondHalf" => PropType::Team,
    "CCSTeam.m_scoreOvertime" => PropType::Team,
    "CCSTeam.m_szClanTeamname" => PropType::Team,
    // RULES
    "CCSGameRulesProxy.CCSGameRules.m_bFreezePeriod"=> PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_bWarmupPeriod" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_fWarmupPeriodEnd" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_fWarmupPeriodStart" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_bTerroristTimeOutActive" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_bCTTimeOutActive" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_flTerroristTimeOutRemaining" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_flCTTimeOutRemaining" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_nTerroristTimeOuts" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_nCTTimeOuts" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_bTechnicalTimeOut" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_bMatchWaitingForResume" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_iRoundTime" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_fMatchStartTime" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_fRoundStartTime" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_flRestartRoundTime" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_bGameRestart" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_flGameStartTime" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_timeUntilNextPhaseStarts" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_gamePhase" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_totalRoundsPlayed" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_nRoundsPlayedThisPhase" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_nOvertimePlaying" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_iHostagesRemaining" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_bAnyHostageReached" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_bMapHasBombTarget" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_bMapHasRescueZone" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_bMapHasBuyZone" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_bIsQueuedMatchmaking" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_nQueuedMatchmakingMode" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_bIsValveDS" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_bLogoMap" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_bPlayAllStepSoundsOnServer" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_iNumGunGameProgressiveWeaponsCT" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_iNumGunGameProgressiveWeaponsT" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_iSpectatorSlotCount" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_GGProgressiveWeaponOrderCT" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_GGProgressiveWeaponOrderT" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_GGProgressiveWeaponKillUpgradeOrderCT" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_GGProgressiveWeaponKillUpgradeOrderT" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_MatchDevice" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_bHasMatchStarted" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_flDMBonusStartTime" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_flDMBonusTimeLength" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_unDMBonusWeaponLoadoutSlot" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_bDMBonusActive" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_nNextMapInMapgroup" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_szTournamentEventName" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_szTournamentEventStage" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_szMatchStatTxt" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_szTournamentPredictionsTxt" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_nTournamentPredictionsPct" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_flCMMItemDropRevealStartTime" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_flCMMItemDropRevealEndTime" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_bIsDroppingItems" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_bIsQuestEligible" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_nGuardianModeWaveNumber" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_nGuardianModeSpecialKillsRemaining" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_nGuardianModeSpecialWeaponNeeded" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_numGlobalGiftsGiven" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_numGlobalGifters" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_numGlobalGiftsPeriodSeconds" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_arrFeaturedGiftersAccounts" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_arrFeaturedGiftersGifts" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_arrProhibitedItemIndices" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_arrTournamentActiveCasterAccounts" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_numBestOfMaps" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_nHalloweenMaskListSeed" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_bBombDropped" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_bBombPlanted" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_iRoundWinStatus" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_eRoundWinReason" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_bTCantBuy" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_bCTCantBuy" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_flGuardianBuyUntilTime" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_iMatchStats_RoundResults" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_iMatchStats_PlayersAlive_CT" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_iMatchStats_PlayersAlive_T" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_TeamRespawnWaveTimes" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_flNextRespawnWave" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_nServerQuestID" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_nEndMatchMapGroupVoteTypes" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_nEndMatchMapGroupVoteOptions" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_nEndMatchMapVoteWinner" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_iNumConsecutiveCTLoses" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_iNumConsecutiveTerroristLoses" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_vecPlayAreaMins" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_vecPlayAreaMaxs" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_iPlayerSpawnHexIndices" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_SpawnTileState" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_flSpawnSelectionTimeStartCurrentStage" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_flSpawnSelectionTimeEndCurrentStage" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_flSpawnSelectionTimeEndLastStage" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_spawnStage" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_flTabletHexOriginX" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_flTabletHexOriginY" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_flTabletHexSize" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_roundData_playerXuids" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_roundData_playerPositions" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_roundData_playerTeams" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_SurvivalGameRuleDecisionTypes" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_SurvivalGameRuleDecisionValues" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_flSurvivalStartTime" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_nMatchSeed" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_bBlockersPresent" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_bRoundInProgress" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_iFirstSecondHalfRound" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_iBombSite" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_nMatchEndCount" => PropType::Rules,
    "CCSGameRulesProxy.CCSGameRules.m_bTeamIntroPeriod" => PropType::Rules,
    // PLAYER CONTROLLER
    "CCSPlayerController.m_bHasCommunicationAbuseMute"=> PropType::Controller,
    "CCSPlayerController.m_szCrosshairCodes" => PropType::Controller,
    "CCSPlayerController.m_iPendingTeamNum" => PropType::Controller,
    "CCSPlayerController.m_flForceTeamTime" => PropType::Controller,
    "CCSPlayerController.m_iCompTeammateColor" => PropType::Controller,
    "CCSPlayerController.m_bEverPlayedOnTeam" => PropType::Controller,
    "CCSPlayerController.m_szClan" => PropType::Controller,
    "CCSPlayerController.m_iCoachingTeam" => PropType::Controller,
    "CCSPlayerController.m_nPlayerDominated" => PropType::Controller,
    "CCSPlayerController.m_nPlayerDominatingMe" => PropType::Controller,
    "CCSPlayerController.m_iCompetitiveRanking" => PropType::Controller,
    "CCSPlayerController.m_iCompetitiveWins" => PropType::Controller,
    "CCSPlayerController.m_iCompetitiveRankType" => PropType::Controller,
    "CCSPlayerController.m_nEndMatchNextMapVote" => PropType::Controller,
    "CCSPlayerController.m_unActiveQuestId" => PropType::Controller,
    "CCSPlayerController.m_nQuestProgressReason" => PropType::Controller,
    "CCSPlayerController.m_unPlayerTvControlFlags" => PropType::Controller,
    "CCSPlayerController.m_nDisconnectionTick" => PropType::Controller,
    "CCSPlayerController.m_bControllingBot" => PropType::Controller,
    "CCSPlayerController.m_bHasControlledBotThisRound" => PropType::Controller,
    "CCSPlayerController.m_bCanControlObservedBot" => PropType::Controller,
    "CCSPlayerController.m_hPlayerPawn" => PropType::Controller,
    "CCSPlayerController.m_hObserverPawn" => PropType::Controller,
    "CCSPlayerController.m_bPawnIsAlive" => PropType::Controller,
    "CCSPlayerController.m_iPawnHealth" => PropType::Controller,
    "CCSPlayerController.m_iPawnArmor" => PropType::Controller,
    "CCSPlayerController.m_bPawnHasDefuser" => PropType::Controller,
    "CCSPlayerController.m_bPawnHasHelmet" => PropType::Controller,
    "CCSPlayerController.m_nPawnCharacterDefIndex" => PropType::Controller,
    "CCSPlayerController.m_iPawnLifetimeStart" => PropType::Controller,
    "CCSPlayerController.m_iPawnLifetimeEnd" => PropType::Controller,
    "CCSPlayerController.m_iPawnGunGameLevel" => PropType::Controller,
    "CCSPlayerController.m_iPawnBotDifficulty" => PropType::Controller,
    "CCSPlayerController.m_hOriginalControllerOfCurrentPawn" => PropType::Controller,
    "CCSPlayerController.m_iScore" => PropType::Controller,
    "CCSPlayerController.m_flSimulationTime" => PropType::Controller,
    "CCSPlayerController.m_nTickBase" => PropType::Controller,
    "CCSPlayerController.m_fFlags" => PropType::Controller,
    "CCSPlayerController.CEntityIdentity.m_nameStringableIndex" => PropType::Controller,
    "CCSPlayerController.m_flCreateTime" => PropType::Controller,
    "CCSPlayerController.m_iTeamNum" => PropType::Controller,
    "CCSPlayerController.m_bSimulatedEveryTick" => PropType::Controller,
    "CCSPlayerController.m_hPawn" => PropType::Controller,
    "CCSPlayerController.m_iConnected" => PropType::Controller,
    "CCSPlayerController.m_iszPlayerName" => PropType::Controller,
    "CCSPlayerController.m_steamID" => PropType::Controller,
    "CCSPlayerController.m_iDesiredFOV" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_InGameMoneyServices.m_iAccount" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_InGameMoneyServices.m_iStartAccount" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_InGameMoneyServices.m_iTotalCashSpent" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_InGameMoneyServices.m_iCashSpentThisRound" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_InventoryServices.m_unMusicID" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_InventoryServices.m_rank" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_InventoryServices.m_nPersonaDataPublicLevel" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_InventoryServices.m_nPersonaDataPublicCommendsLeader" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_InventoryServices.m_nPersonaDataPublicCommendsTeacher" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_InventoryServices.m_nPersonaDataPublicCommendsFriendly" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_InventoryServices.CEconItemView.m_iItemDefinitionIndex" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_InventoryServices.CEconItemView.m_iEntityQuality" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_InventoryServices.CEconItemView.m_iEntityLevel" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_InventoryServices.CEconItemView.m_iItemIDHigh" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_InventoryServices.CEconItemView.m_iItemIDLow" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_InventoryServices.CEconItemView.m_iAccountID" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_InventoryServices.CEconItemView.m_iInventoryPosition" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_InventoryServices.CEconItemView.m_bInitialized" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_InventoryServices.CEconItemView.CEconItemAttribute.m_iAttributeDefinitionIndex" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_InventoryServices.CEconItemView.CEconItemAttribute.m_iRawValue32" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_InventoryServices.CEconItemView.CEconItemAttribute.m_flInitialValue" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_InventoryServices.CEconItemView.CEconItemAttribute.m_nRefundableCurrency" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_InventoryServices.CEconItemView.CEconItemAttribute.m_bSetBonus" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_InventoryServices.CEconItemView.m_szCustomName" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.CSPerRoundStats_t.m_iKills" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.CSPerRoundStats_t.m_iDeaths" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.CSPerRoundStats_t.m_iAssists" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.CSPerRoundStats_t.m_iLiveTime" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.CSPerRoundStats_t.m_iHeadShotKills" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.CSPerRoundStats_t.m_iDamage" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.CSPerRoundStats_t.m_iObjective" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.CSPerRoundStats_t.m_iUtilityDamage" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.CSPerRoundStats_t.m_iEnemiesFlashed" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.CSPerRoundStats_t.m_iEquipmentValue" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.CSPerRoundStats_t.m_iMoneySaved" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.CSPerRoundStats_t.m_iKillReward" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.CSPerRoundStats_t.m_iCashEarned" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.m_iKills" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.m_iDeaths" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.m_iAssists" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.m_iLiveTime" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.m_iHeadShotKills" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.m_iEnemy5Ks" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.m_iEnemy4Ks" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.m_iEnemy3Ks" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.m_iNumRoundKills" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.m_iNumRoundKillsHeadshots" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.m_iDamage" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.m_iObjective" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.m_iUtilityDamage" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.m_iEnemiesFlashed" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.m_iEquipmentValue" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.m_iMoneySaved" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.m_iKillReward" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_ActionTrackingServices.m_iCashEarned" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_DamageServices.m_nSendUpdate" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_DamageServices.CDamageRecord.m_PlayerDamager" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_DamageServices.CDamageRecord.m_PlayerRecipient" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_DamageServices.CDamageRecord.m_hPlayerControllerDamager" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_DamageServices.CDamageRecord.m_hPlayerControllerRecipient" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_DamageServices.CDamageRecord.m_szPlayerDamagerName" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_DamageServices.CDamageRecord.m_szPlayerRecipientName" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_DamageServices.CDamageRecord.m_DamagerXuid" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_DamageServices.CDamageRecord.m_RecipientXuid" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_DamageServices.CDamageRecord.m_iDamage" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_DamageServices.CDamageRecord.m_iActualHealthRemoved" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_DamageServices.CDamageRecord.m_iNumHits" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_DamageServices.CDamageRecord.m_iLastBulletUpdate" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_DamageServices.CDamageRecord.m_bIsOtherEnemy" => PropType::Controller,
    "CCSPlayerController.CCSPlayerController_DamageServices.CDamageRecord.m_killType" => PropType::Controller,
    "CCSPlayerController.m_iPing"=> PropType::Controller,

    "CCSPlayerPawn.m_MoveCollide" => PropType::Player,
    "CCSPlayerPawn.m_MoveType" => PropType::Player,
    "CCSPlayerPawn.m_iTeamNum" => PropType::Player,
    "CCSPlayerPawn.CCSPlayer_WeaponServices.m_hActiveWeapon" => PropType::Player,
    "CCSPlayerPawn.CCSPlayer_WeaponServices.m_iAmmo" => PropType::Player,
    "CCSPlayerPawn.CCSPlayer_WeaponServices.m_bIsLookingAtWeapon" => PropType::Player,
    "CCSPlayerPawn.CCSPlayer_WeaponServices.m_bIsHoldingLookAtWeapon" => PropType::Player,
    "CCSPlayerPawn.CCSPlayer_WeaponServices.m_flNextAttack" => PropType::Player,
    "CCSPlayerPawn.CCSPlayer_MovementServices.m_nDuckTimeMsecs" => PropType::Player,
    "CCSPlayerPawn.CCSPlayer_MovementServices.m_flMaxspeed" => PropType::Player,
    "CCSPlayerPawn.CCSPlayer_MovementServices.m_flMaxFallVelocity" => PropType::Player,
    "CCSPlayerPawn.CCSPlayer_MovementServices.m_flDuckAmount" => PropType::Player,
    "CCSPlayerPawn.CCSPlayer_MovementServices.m_flDuckSpeed" => PropType::Player,
    "CCSPlayerPawn.CCSPlayer_MovementServices.m_bDuckOverride" => PropType::Player,
    "CCSPlayerPawn.CCSPlayer_MovementServices.m_bOldJumpPressed" => PropType::Player,
    "CCSPlayerPawn.CCSPlayer_MovementServices.m_flJumpUntil" => PropType::Player,
    "CCSPlayerPawn.CCSPlayer_MovementServices.m_flJumpVel" => PropType::Player,
    "CCSPlayerPawn.CCSPlayer_MovementServices.m_flFallVelocity" => PropType::Player,
    "CCSPlayerPawn.CCSPlayer_MovementServices.m_bInCrouch" => PropType::Player,
    "CCSPlayerPawn.CCSPlayer_MovementServices.m_nCrouchState" => PropType::Player,
    "CCSPlayerPawn.CCSPlayer_MovementServices.m_bDucked" => PropType::Player,
    "CCSPlayerPawn.CCSPlayer_MovementServices.m_bDucking" => PropType::Player,
    "CCSPlayerPawn.CCSPlayer_MovementServices.m_bInDuckJump" => PropType::Player,
    "CCSPlayerPawn.CCSPlayer_MovementServices.m_bAllowAutoMovement" => PropType::Player,
    "CCSPlayerPawn.CCSPlayer_MovementServices.m_nJumpTimeMsecs" => PropType::Player,
    "CCSPlayerPawn.CCSPlayer_MovementServices.m_flLastDuckTime" => PropType::Player,
    "CCSPlayerPawn.CCSPlayer_ActionTrackingServices.m_bIsRescuing" => PropType::Player,
    "CCSPlayerPawn.CCSPlayer_ActionTrackingServices.m_iWeaponPurchasesThisMatch" => PropType::Player,
    "CCSPlayerPawn.CCSPlayer_ActionTrackingServices.m_iWeaponPurchasesThisRound" => PropType::Player,
    "CCSPlayerPawn.m_bSpotted" => PropType::Player,
    "CCSPlayerPawn.m_bSpottedByMask" => PropType::Player,
    "CCSPlayerPawn.m_flTimeOfLastInjury" => PropType::Player,
    "CCSPlayerPawn.m_nRelativeDirectionOfLastInjury" => PropType::Player,
    "CCSPlayerPawn.m_iPlayerState" => PropType::Player,
    "CCSPlayerPawn.m_passiveItems" => PropType::Player,
    "CCSPlayerPawn.m_bIsScoped" => PropType::Player,
    "CCSPlayerPawn.m_bIsWalking" => PropType::Player,
    "CCSPlayerPawn.m_bResumeZoom" => PropType::Player,
    "CCSPlayerPawn.m_bIsDefusing" => PropType::Player,
    "CCSPlayerPawn.m_bIsGrabbingHostage" => PropType::Player,
    "CCSPlayerPawn.m_iBlockingUseActionInProgress" => PropType::Player,
    "CCSPlayerPawn.m_fMolotovDamageTime" => PropType::Player,
    "CCSPlayerPawn.m_bHasMovedSinceSpawn" => PropType::Player,
    "CCSPlayerPawn.m_bInBombZone" => PropType::Player,
    "CCSPlayerPawn.m_bInBuyZone" => PropType::Player,
    "CCSPlayerPawn.m_bInNoDefuseArea" => PropType::Player,
    "CCSPlayerPawn.m_bKilledByTaser" => PropType::Player,
    "CCSPlayerPawn.m_iMoveState" => PropType::Player,
    "CCSPlayerPawn.m_nWhichBombZone" => PropType::Player,
    "CCSPlayerPawn.m_bInHostageRescueZone" => PropType::Player,
    "CCSPlayerPawn.m_flStamina" => PropType::Player,
    "CCSPlayerPawn.m_iDirection" => PropType::Player,
    "CCSPlayerPawn.m_iShotsFired" => PropType::Player,
    "CCSPlayerPawn.m_ArmorValue" => PropType::Player,
    "CCSPlayerPawn.m_flVelocityModifier" => PropType::Player,
    "CCSPlayerPawn.m_flGroundAccelLinearFracLastTime" => PropType::Player,
    "CCSPlayerPawn.m_flFlashDuration" => PropType::Player,
    "CCSPlayerPawn.m_flFlashMaxAlpha" => PropType::Player,
    "CCSPlayerPawn.m_bWaitForNoAttack" => PropType::Player,
    "CCSPlayerPawn.m_szLastPlaceName" => PropType::Player,
    "CCSPlayerPawn.m_bStrafing" => PropType::Player,
    "CCSPlayerPawn.m_unRoundStartEquipmentValue" => PropType::Player,
    "CCSPlayerPawn.m_unCurrentEquipmentValue" => PropType::Player,
    "CCSPlayerPawn.m_flSimulationTime" => PropType::Player,
    "CCSPlayerPawn.m_iHealth" => PropType::Player,
    "CCSPlayerPawn.m_lifeState" => PropType::Player,
    // Custom
    "X"=> PropType::Custom,
    "Y"=> PropType::Custom,
    "Z"=> PropType::Custom,
};